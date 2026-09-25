//! Turns a CurseForge or Feed the Beast modpack into a `.mrpack` that the
//! regular modpack installer installs, so these packs get the same install
//! jobs, progress, content indexing and retry as Modrinth modpacks.

use crate::State;
use crate::install::{InstallJobSnapshot, InstallPostInstallEdit};
use crate::pack::install_from::{CreatePackLocation, PackDependency};
use crate::state::ModLoader;
use crate::util::fetch::{
    DownloadMeta, DownloadedFile, FetchProgressFn, fetch_content_file,
};
use path_util::SafeRelativeUtf8UnixPathBuf;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Built packs are kept a while so a failed install can be retried.
const KEEP_BUILT_PACKS: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Downloads a modpack file into the content store. Modrinth packs give
/// SHA-512 hashes; CurseForge and Feed the Beast files only have SHA-1s, so
/// those are checked against that instead.
pub(crate) async fn fetch_pack_content_file(
    state: &State,
    mirrors: &[&str],
    sha512: Option<&str>,
    sha1: Option<&str>,
    size: Option<u64>,
    download_meta: Option<&DownloadMeta>,
    progress: Option<&mut FetchProgressFn<'_>>,
) -> crate::Result<DownloadedFile> {
    if sha512.is_some() {
        return fetch_content_file(
            state,
            mirrors,
            sha512,
            size,
            download_meta,
            progress,
        )
        .await;
    }
    let acquired = state
        .content_store
        .download_file_by_sha1(
            mirrors,
            sha1,
            download_meta,
            &state.fetch_semaphore,
            progress,
        )
        .await?;
    Ok(DownloadedFile::from_stored_file(
        acquired.stored_file,
        acquired.reused,
    ))
}

/// A file the pack downloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BundleFile {
    /// Where it goes in the instance, like `mods/jei.jar`.
    pub path: String,
    pub urls: Vec<String>,
    pub sha1: Option<String>,
    pub size: u64,
}

/// A file that has to be downloaded by hand, because its author doesn't allow
/// downloads through other apps.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ManualDownload {
    pub name: String,
    /// The page to download it from.
    pub url: String,
    /// The folder of the instance it belongs in, like `mods`.
    pub folder: String,
}

/// What starting a modpack install did.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackInstallReport {
    pub job: InstallJobSnapshot,
    pub instance_name: String,
    /// Files left out of the instance: download these and put them in.
    pub manual_downloads: Vec<ManualDownload>,
}

/// Overrides copied from another zip: every entry under `prefix`.
pub(crate) struct ZipOverrides {
    pub archive: PathBuf,
    pub prefix: String,
}

pub(crate) struct Bundle {
    pub name: String,
    pub version: String,
    pub summary: Option<String>,
    pub game_version: String,
    pub loader: ModLoader,
    pub loader_version: Option<String>,
    pub files: Vec<BundleFile>,
    pub overrides: Option<ZipOverrides>,
}

/// The loader in a CurseForge manifest's `modLoaders[].id`, like
/// `forge-47.2.0`, `neoforge-20.4.80-beta` or `fabric-0.15.0`.
pub(crate) fn parse_loader_id(id: &str) -> Option<(ModLoader, String)> {
    let (name, version) = id.split_once('-')?;
    let loader = match name.to_ascii_lowercase().as_str() {
        "forge" => ModLoader::Forge,
        "neoforge" => ModLoader::NeoForge,
        "fabric" => ModLoader::Fabric,
        "quilt" => ModLoader::Quilt,
        _ => return None,
    };
    (!version.is_empty()).then(|| (loader, version.to_string()))
}

/// The id the launcher metadata uses for a loader version written another
/// way, like `1.20.1-47.1.106` or `1.7.10-10.13.4.1614-1.7.10` for
/// `47.1.106` and `10.13.4.1614`. Unknown versions stay as they are.
pub(crate) fn match_loader_version<'a>(
    game_version: &str,
    version: &str,
    known: impl IntoIterator<Item = &'a str> + Clone,
) -> String {
    if known.clone().into_iter().any(|id| id == version) {
        return version.to_string();
    }
    let trimmed = version
        .strip_prefix(&format!("{game_version}-"))
        .unwrap_or(version);
    let trimmed = trimmed
        .strip_suffix(&format!("-{game_version}"))
        .unwrap_or(trimmed);
    known
        .into_iter()
        .find(|id| *id == trimmed)
        .map(str::to_string)
        .unwrap_or_else(|| trimmed.to_string())
}

async fn normalize_loader_version(
    game_version: &str,
    loader: ModLoader,
    version: &str,
) -> String {
    let manifest =
        match crate::api::metadata::get_loader_versions(loader.as_meta_str())
            .await
        {
            Ok(manifest) => manifest,
            Err(error) => {
                tracing::warn!("Couldn't read {loader:?} versions: {error}");
                return version.to_string();
            }
        };
    let placeholder = daedalus::modded::DUMMY_REPLACE_STRING;
    let Some(entry) = manifest
        .game_versions
        .iter()
        .find(|x| x.id.replace(placeholder, game_version) == game_version)
    else {
        return version.to_string();
    };
    let loaders = match &entry.version_group {
        Some(group) => manifest
            .version_groups
            .iter()
            .find(|x| x.id == *group)
            .map(|x| x.loaders.as_slice())
            .unwrap_or_default(),
        None => entry.loaders.as_slice(),
    };
    match_loader_version(
        game_version,
        version,
        loaders.iter().map(|x| x.id.as_str()),
    )
}

/// A file name that is safe to put in an instance folder.
pub(crate) fn safe_file_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':', '\0']))
    .then(|| name.to_string())
}

/// A relative path inside the instance, or `None` if it leaves it.
pub(crate) fn safe_relative_path(path: &str) -> Option<String> {
    let parts = path
        .replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(|part| part == "..") {
        return None;
    }
    let joined = parts.join("/");
    SafeRelativeUtf8UnixPathBuf::try_from(joined.clone())
        .ok()
        .map(|_| joined)
}

fn pack_dependencies(bundle: &Bundle) -> HashMap<PackDependency, String> {
    let mut dependencies = HashMap::new();
    dependencies.insert(PackDependency::Minecraft, bundle.game_version.clone());
    let loader = match bundle.loader {
        ModLoader::Forge => Some(PackDependency::Forge),
        ModLoader::NeoForge => Some(PackDependency::NeoForge),
        ModLoader::Fabric => Some(PackDependency::FabricLoader),
        ModLoader::Quilt => Some(PackDependency::QuiltLoader),
        ModLoader::Vanilla => None,
    };
    if let (Some(loader), Some(version)) = (loader, &bundle.loader_version) {
        dependencies.insert(loader, version.clone());
    }
    dependencies
}

/// The `modrinth.index.json` of the bundle. Files with the same path are
/// only listed once.
pub(crate) fn pack_index(bundle: &Bundle) -> serde_json::Value {
    let mut seen = HashSet::new();
    let files = bundle
        .files
        .iter()
        .filter(|file| seen.insert(file.path.to_ascii_lowercase()))
        .map(|file| {
            let mut hashes = serde_json::Map::new();
            if let Some(sha1) = &file.sha1 {
                hashes.insert("sha1".to_string(), sha1.clone().into());
            }
            serde_json::json!({
                "path": file.path,
                "hashes": hashes,
                "downloads": file.urls,
                "fileSize": u32::try_from(file.size).unwrap_or(u32::MAX),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "game": "minecraft",
        "formatVersion": 1,
        "versionId": bundle.version,
        "name": bundle.name,
        "summary": bundle.summary,
        "files": files,
        "dependencies": pack_dependencies(bundle),
    })
}

/// Writes the bundle as an `.mrpack`. Blocking.
pub(crate) fn write_mrpack(bundle: &Bundle, out: &Path) -> crate::Result<()> {
    let index = serde_json::to_vec_pretty(&pack_index(bundle))?;
    let file = std::fs::File::create(out)
        .map_err(|e| crate::util::io::IOError::with_path(e, out))?;
    let mut zip = ZipWriter::new(std::io::BufWriter::new(file));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true);
    zip.start_file("modrinth.index.json", options)
        .map_err(std::io::Error::from)?;
    zip.write_all(&index)?;

    if let Some(overrides) = &bundle.overrides {
        let source = std::fs::File::open(&overrides.archive).map_err(|e| {
            crate::util::io::IOError::with_path(e, &overrides.archive)
        })?;
        copy_overrides(
            &mut ZipArchive::new(std::io::BufReader::new(source))
                .map_err(std::io::Error::from)?,
            &overrides.prefix,
            &mut zip,
            options,
        )?;
    }
    zip.finish().map_err(std::io::Error::from)?.flush()?;
    Ok(())
}

fn copy_overrides<R: Read + Seek, W: Write + Seek>(
    source: &mut ZipArchive<R>,
    prefix: &str,
    zip: &mut ZipWriter<W>,
    options: SimpleFileOptions,
) -> crate::Result<()> {
    let prefix = format!("{}/", prefix.trim_matches('/'));
    for index in 0..source.len() {
        let mut entry = source.by_index(index).map_err(std::io::Error::from)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let Some(relative) = name.strip_prefix(&prefix) else {
            continue;
        };
        let Some(relative) = safe_relative_path(relative) else {
            tracing::warn!("Skipping unsafe override path {name}");
            continue;
        };
        zip.start_file(format!("overrides/{relative}"), options)
            .map_err(std::io::Error::from)?;
        std::io::copy(&mut entry, zip)?;
    }
    Ok(())
}

fn packs_dir(state: &State) -> PathBuf {
    state.directories.caches_dir().join("external-packs")
}

/// Removes built packs old enough that no install needs them any more.
async fn clean_old_packs(dir: &Path) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let old = entry
            .metadata()
            .await
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| {
                SystemTime::now().duration_since(modified).ok()
            })
            .is_some_and(|age| age > KEEP_BUILT_PACKS);
        if old {
            let _ = tokio::fs::remove_dir_all(entry.path()).await;
        }
    }
}

/// Builds the bundle's `.mrpack` in the app cache and returns its path.
pub(crate) async fn build(bundle: Bundle) -> crate::Result<PathBuf> {
    let state = State::get().await?;
    let dir = packs_dir(&state);
    clean_old_packs(&dir).await;
    let dir = dir.join(uuid::Uuid::new_v4().to_string());
    crate::util::io::create_dir_all(&dir).await?;
    // The install job is named after the file.
    let file_name = safe_file_name(&bundle.name.replace(['/', '\\', ':'], " "))
        .unwrap_or_else(|| "Modpack".to_string());
    let out = dir.join(format!("{file_name}.mrpack"));

    let mut bundle = bundle;
    if let (Some(version), true) =
        (&bundle.loader_version, bundle.loader != ModLoader::Vanilla)
    {
        bundle.loader_version = Some(
            normalize_loader_version(
                &bundle.game_version,
                bundle.loader,
                version,
            )
            .await,
        );
    }

    let path = out.clone();
    tokio::task::spawn_blocking(move || write_mrpack(&bundle, &path))
        .await
        .map_err(|error| {
            crate::ErrorKind::OtherError(format!(
                "Couldn't build the modpack: {error}"
            ))
        })??;
    Ok(out)
}

/// Downloads the pack's icon ahead of the install, so a broken icon link
/// doesn't fail it.
async fn cache_icon(url: &str) -> Option<String> {
    let state = State::get().await.ok()?;
    let result = async {
        let bytes = crate::util::fetch::fetch(
            url,
            None,
            None,
            None,
            &state.fetch_semaphore,
            &state.pool,
        )
        .await?;
        crate::api::instance::cache_icon(bytes, &state).await
    }
    .await;
    match result {
        Ok(path) => Some(path.to_string_lossy().to_string()),
        Err(error) => {
            tracing::warn!("Couldn't download the modpack icon {url}: {error}");
            None
        }
    }
}

/// Builds the bundle and starts installing it as a new instance.
pub(crate) async fn install(
    bundle: Bundle,
    icon_url: Option<String>,
    manual_downloads: Vec<ManualDownload>,
) -> crate::Result<PackInstallReport> {
    let instance_name = bundle.name.clone();
    let path = build(bundle).await?;
    let icon_path = match icon_url {
        Some(url) => cache_icon(&url).await,
        None => None,
    };
    let job = crate::install::create_modpack_instance(
        CreatePackLocation::FromFile { path },
        Some(InstallPostInstallEdit {
            name: Some(instance_name.clone()),
            icon_path: icon_path.map(Some),
            link: None,
        }),
    )
    .await?;
    Ok(PackInstallReport {
        job,
        instance_name,
        manual_downloads,
    })
}
