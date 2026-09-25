//! Exports an instance as a ready-to-run server: the mods a server needs,
//! their configs, the mod loader's server launcher and start scripts.

use super::export_mrpack::{
    ExportSelection, export_content, is_path_exportable, pack_get_relative_path,
};
use super::{create_mrpack_json, get, get_full_path};
use crate::pack::install_from::EnvType;
use crate::state::content_store::{ReadableContent, content_file_path};
use crate::state::instances::adapters::sqlite::content_rows;
use crate::state::{InstanceMetadata, ModLoader, SideType, State};
use crate::util::fetch::{fetch, fetch_json};
use crate::util::io::{self, IOError};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Folders selected for the server pack by default, besides `mods`. Worlds,
/// resource packs, shaders, screenshots and client options stay out unless
/// picked.
const SERVER_FOLDERS: &[&str] = &[
    "config",
    "defaultconfigs",
    "kubejs",
    "scripts",
    "global_packs",
    "datapacks",
];

/// The paths selected when the export screen opens, as include and exclude
/// rules like the modpack export uses: `mods` and the server folders, minus
/// the mods Modrinth lists as client-only and disabled mods.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct ServerPackSelection {
    pub included: Vec<String>,
    pub excluded: Vec<String>,
}

/// What the export put in.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ServerPackReport {
    pub mods_included: usize,
}

pub(crate) enum Entry {
    Content(ReadableContent),
    Bytes(Vec<u8>, bool),
}

/// The instance with its mod loader version filled in the way launching
/// resolves it, for instances that don't record one (like folders added by
/// hand).
async fn with_loader_version(
    instance_id: &str,
) -> crate::Result<InstanceMetadata> {
    let mut metadata = get(instance_id).await?.ok_or_else(|| {
        crate::ErrorKind::InputError("Unknown instance".to_string())
    })?;
    let content_set = &mut metadata.applied_content_set;
    if content_set.loader != ModLoader::Vanilla {
        let mut version = crate::launcher::get_loader_version_from_profile(
            &content_set.game_version,
            content_set.loader,
            content_set.loader_version.as_deref(),
        )
        .await?;
        if version.is_none() {
            version = crate::launcher::get_loader_version_from_profile(
                &content_set.game_version,
                content_set.loader,
                Some("stable"),
            )
            .await?;
        }
        content_set.loader_version =
            Some(version.map(|version| version.id).ok_or_else(|| {
                crate::ErrorKind::InputError(format!(
                    "No {} version found for Minecraft {}",
                    content_set.loader.as_meta_str(),
                    content_set.game_version
                ))
            })?);
    }
    Ok(metadata)
}

/// The default selection for the server pack export screen.
#[tracing::instrument]
pub async fn server_pack_selection(
    instance_id: &str,
) -> crate::Result<ServerPackSelection> {
    let state = State::get().await?;
    crate::state::instances::commands::sync_content_files(instance_id, &state)
        .await?;
    let metadata = with_loader_version(instance_id).await?;
    let instance_dir = get_full_path(instance_id).await?;

    // Modrinth tells which mods are client-only.
    let pack = create_mrpack_json(&metadata, "1.0.0".to_string(), None).await?;
    let mut excluded = pack
        .files
        .iter()
        .filter(|file| {
            file.env.as_ref().and_then(|env| env.get(&EnvType::Server))
                == Some(&SideType::Unsupported)
        })
        .map(|file| file.path.as_str().to_string())
        .collect::<Vec<_>>();
    if let Ok(mut mods) = io::read_dir(instance_dir.join("mods")).await {
        while let Ok(Some(entry)) = mods.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".disabled") {
                excluded.push(format!("mods/{name}"));
            }
        }
    }
    excluded.sort();
    excluded.dedup();

    let included = std::iter::once("mods")
        .chain(SERVER_FOLDERS.iter().copied())
        .filter(|folder| instance_dir.join(folder).is_dir())
        .map(str::to_string)
        .collect();
    Ok(ServerPackSelection { included, excluded })
}

/// Writes the server pack for `instance_id` as a zip to `export_path`: the
/// selected files (include and exclude rules, like the modpack export), the
/// mod loader's server launcher and start scripts.
#[tracing::instrument]
pub async fn export_server_pack(
    instance_id: &str,
    export_path: PathBuf,
    included: Vec<String>,
    excluded: Vec<String>,
) -> crate::Result<ServerPackReport> {
    let state = State::get().await?;
    if let Some(parent) = export_path.parent() {
        let parent = tokio::fs::canonicalize(parent).await?;
        if parent.starts_with(
            tokio::fs::canonicalize(state.directories.instances_dir()).await?,
        ) {
            return Err(crate::ErrorKind::InputError(
                "Save the server pack outside the instances folder".to_string(),
            )
            .into());
        }
    }

    let files = collect_server_files(instance_id, included, excluded).await?;
    let entries = files.entries;
    tokio::task::spawn_blocking(move || write_zip(&export_path, entries))
        .await??;
    Ok(files.report)
}

/// Everything a server pack contains, ready to be written as a zip or into a
/// server folder.
pub(crate) struct ServerFiles {
    /// The instance, with its mod loader version filled in.
    pub metadata: InstanceMetadata,
    pub report: ServerPackReport,
    pub entries: Vec<(String, Entry)>,
    /// The launcher (or installer) jar among the entries.
    pub launcher_file: String,
}

/// Collects the selected files of an instance plus the mod loader's server
/// launcher, start scripts and README.
pub(crate) async fn collect_server_files(
    instance_id: &str,
    included: Vec<String>,
    excluded: Vec<String>,
) -> crate::Result<ServerFiles> {
    let state = State::get().await?;
    let metadata = with_loader_version(instance_id).await?;
    let instance_dir = get_full_path(instance_id).await?;
    let selection = ExportSelection::new(included, excluded);
    let stored_files =
        content_rows::get_instance_files(instance_id, &state.pool)
            .await?
            .into_iter()
            .map(|file| (content_file_path(&file), file))
            .collect::<HashMap<_, _>>();

    let mut report = ServerPackReport::default();
    let mut entries: Vec<(String, Entry)> = Vec::new();
    let mut pending = vec![instance_dir.clone()];
    while let Some(dir) = pending.pop() {
        let mut read_dir = io::read_dir(&dir).await?;
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|error| IOError::with_path(error, &dir))?
        {
            let path = entry.path();
            let relative = pack_get_relative_path(&instance_dir, &path)?;
            if !is_path_exportable(&relative) {
                continue;
            }
            let file_type = entry
                .file_type()
                .await
                .map_err(|error| IOError::with_path(error, &path))?;
            if file_type.is_dir() {
                if selection.should_visit_directory(&relative) {
                    pending.push(path);
                }
                continue;
            }
            // Disabled mods can't load on a server either.
            if !selection.is_included(&relative)
                || relative.as_str().ends_with(".disabled")
            {
                continue;
            }
            let relative = relative.as_str().to_string();
            if relative.starts_with("mods/")
                && relative.ends_with(".jar")
                && relative.matches('/').count() == 1
            {
                report.mods_included += 1;
            }
            let Some(content) = export_content(
                &state,
                stored_files.get(relative.as_str()),
                &path,
                file_type.is_symlink(),
            )
            .await?
            else {
                continue;
            };
            entries.push((relative, Entry::Content(content)));
        }
    }

    let content_set = &metadata.applied_content_set;
    let launcher = server_launcher(
        &state,
        &content_set.game_version,
        content_set.loader,
        content_set.loader_version.as_deref(),
    )
    .await?;
    let launcher_file = launcher.file_name.clone();
    entries.push((
        launcher.file_name.clone(),
        Entry::Bytes(launcher.jar, false),
    ));
    entries.push((
        "start.sh".to_string(),
        Entry::Bytes(launcher.start_sh.into_bytes(), true),
    ));
    entries.push((
        "start.bat".to_string(),
        Entry::Bytes(launcher.start_bat.into_bytes(), false),
    ));
    entries.push((
        "README.txt".to_string(),
        Entry::Bytes(
            readme(&metadata.instance.name, &content_set.game_version, &report)
                .into_bytes(),
            false,
        ),
    ));

    Ok(ServerFiles {
        metadata,
        report,
        entries,
        launcher_file,
    })
}

/// Writes server pack entries into a folder, replacing files that exist.
pub(crate) fn write_dir(
    dir: &Path,
    entries: Vec<(String, Entry)>,
) -> crate::Result<()> {
    for (name, entry) in entries {
        let target = dir.join(&name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| IOError::with_path(error, parent))?;
        }
        match entry {
            Entry::Content(content) => {
                std::fs::copy(content.path(), &target)
                    .map_err(|error| IOError::with_path(error, &target))?;
            }
            Entry::Bytes(bytes, executable) => {
                std::fs::write(&target, bytes)
                    .map_err(|error| IOError::with_path(error, &target))?;
                #[cfg(unix)]
                if executable {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(
                        &target,
                        std::fs::Permissions::from_mode(0o755),
                    )
                    .map_err(|error| IOError::with_path(error, &target))?;
                }
                #[cfg(not(unix))]
                let _ = executable;
            }
        }
    }
    Ok(())
}

fn write_zip(path: &Path, entries: Vec<(String, Entry)>) -> crate::Result<()> {
    let file = std::fs::File::create(path)
        .map_err(|error| IOError::with_path(error, path))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true);
    let mut buffer = vec![0_u8; 256 * 1024];
    for (name, entry) in entries {
        match entry {
            Entry::Content(content) => {
                zip.start_file(name, options)
                    .map_err(std::io::Error::from)?;
                let source_path = content.path();
                let mut source = std::fs::File::open(source_path)
                    .map_err(|error| IOError::with_path(error, source_path))?;
                loop {
                    let read = source.read(&mut buffer).map_err(|error| {
                        IOError::with_path(error, source_path)
                    })?;
                    if read == 0 {
                        break;
                    }
                    zip.write_all(&buffer[..read])?;
                }
            }
            Entry::Bytes(bytes, executable) => {
                let options = if executable {
                    options.unix_permissions(0o755)
                } else {
                    options
                };
                zip.start_file(name, options)
                    .map_err(std::io::Error::from)?;
                zip.write_all(&bytes)?;
            }
        }
    }
    zip.finish().map_err(std::io::Error::from)?;
    Ok(())
}

struct ServerLauncher {
    file_name: String,
    jar: Vec<u8>,
    start_sh: String,
    start_bat: String,
}

const JAVA_ARGS: &str = "-Xms2G -Xmx4G";

/// The loader's server launcher (or installer) plus start scripts. Installers
/// run once on first start; the scripts skip them afterwards.
async fn server_launcher(
    state: &State,
    game_version: &str,
    loader: ModLoader,
    loader_version: Option<&str>,
) -> crate::Result<ServerLauncher> {
    let needs_version = || {
        loader_version.map(str::to_string).ok_or_else(|| {
            crate::Error::from(crate::ErrorKind::InputError(
                "This instance has no mod loader version set".to_string(),
            ))
        })
    };
    let run_jar = |jar: &str| ServerLauncher {
        file_name: jar.to_string(),
        jar: Vec::new(),
        start_sh: format!(
            "#!/bin/sh\ncd \"$(dirname \"$0\")\"\njava {JAVA_ARGS} -jar {jar} nogui\n"
        ),
        start_bat: format!(
            "@echo off\r\ncd /d \"%~dp0\"\r\njava {JAVA_ARGS} -jar {jar} nogui\r\npause\r\n"
        ),
    };

    let (url, mut launcher) = match loader {
        ModLoader::Vanilla => (
            vanilla_server_url(state, game_version).await?,
            run_jar("server.jar"),
        ),
        ModLoader::Fabric => {
            let installers: Vec<FabricInstaller> = fetch_json(
                Method::GET,
                "https://meta.fabricmc.net/v2/versions/installer",
                None,
                None,
                None,
                &state.fetch_semaphore,
                &state.pool,
            )
            .await?;
            let installer = installers
                .iter()
                .find(|installer| installer.stable)
                .or(installers.first())
                .ok_or_else(|| {
                    crate::ErrorKind::OtherError(
                        "No Fabric installer found".to_string(),
                    )
                })?;
            (
                format!(
                    "https://meta.fabricmc.net/v2/versions/loader/{game_version}/{}/{}/server/jar",
                    needs_version()?,
                    installer.version
                ),
                run_jar("fabric-server-launch.jar"),
            )
        }
        ModLoader::Quilt => {
            let installers: Vec<QuiltInstaller> = fetch_json(
                Method::GET,
                "https://meta.quiltmc.org/v3/versions/installer",
                None,
                None,
                None,
                &state.fetch_semaphore,
                &state.pool,
            )
            .await?;
            let installer = installers.first().ok_or_else(|| {
                crate::ErrorKind::OtherError(
                    "No Quilt installer found".to_string(),
                )
            })?;
            let version = needs_version()?;
            let install = format!(
                "java -jar quilt-installer.jar install server {game_version} {version} --download-server --install-dir=."
            );
            (
                installer.url.clone(),
                ServerLauncher {
                    file_name: "quilt-installer.jar".to_string(),
                    jar: Vec::new(),
                    start_sh: format!(
                        "#!/bin/sh\ncd \"$(dirname \"$0\")\"\n[ -f quilt-server-launch.jar ] || {install} || exit 1\njava {JAVA_ARGS} -jar quilt-server-launch.jar nogui\n"
                    ),
                    start_bat: format!(
                        "@echo off\r\ncd /d \"%~dp0\"\r\nif not exist quilt-server-launch.jar {install}\r\njava {JAVA_ARGS} -jar quilt-server-launch.jar nogui\r\npause\r\n"
                    ),
                },
            )
        }
        ModLoader::Forge | ModLoader::NeoForge => {
            let version = needs_version()?;
            let (url, installer) = if loader == ModLoader::Forge {
                (
                    format!(
                        "https://maven.minecraftforge.net/net/minecraftforge/forge/{game_version}-{version}/forge-{game_version}-{version}-installer.jar"
                    ),
                    "forge-installer.jar",
                )
            } else if game_version == "1.20.1" {
                (
                    format!(
                        "https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-{version}/forge-1.20.1-{version}-installer.jar"
                    ),
                    "neoforge-installer.jar",
                )
            } else {
                (
                    format!(
                        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar"
                    ),
                    "neoforge-installer.jar",
                )
            };
            // The installer creates run.sh/run.bat; memory goes in
            // user_jvm_args.txt, which the scripts fill in on first start.
            (
                url,
                ServerLauncher {
                    file_name: installer.to_string(),
                    jar: Vec::new(),
                    start_sh: format!(
                        "#!/bin/sh\ncd \"$(dirname \"$0\")\"\nif [ ! -f run.sh ]; then\n  java -jar {installer} --installServer || exit 1\n  echo \"{JAVA_ARGS}\" >> user_jvm_args.txt\nfi\nsh run.sh nogui\n"
                    ),
                    start_bat: format!(
                        "@echo off\r\ncd /d \"%~dp0\"\r\nif not exist run.bat (\r\n  java -jar {installer} --installServer\r\n  echo {JAVA_ARGS}>> user_jvm_args.txt\r\n)\r\ncall run.bat nogui\r\npause\r\n"
                    ),
                },
            )
        }
    };

    launcher.jar =
        fetch(&url, None, None, None, &state.fetch_semaphore, &state.pool)
            .await?
            .to_vec();
    Ok(launcher)
}

#[derive(Deserialize)]
struct FabricInstaller {
    version: String,
    stable: bool,
}

#[derive(Deserialize)]
struct QuiltInstaller {
    url: String,
}

#[derive(Deserialize)]
struct VersionManifest {
    versions: Vec<ManifestVersion>,
}

#[derive(Deserialize)]
struct ManifestVersion {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct VersionInfo {
    downloads: VersionDownloads,
}

#[derive(Deserialize)]
struct VersionDownloads {
    server: Option<Download>,
}

#[derive(Deserialize)]
struct Download {
    url: String,
}

async fn vanilla_server_url(
    state: &State,
    game_version: &str,
) -> crate::Result<String> {
    let manifest: VersionManifest = fetch_json(
        Method::GET,
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;
    let version = manifest
        .versions
        .into_iter()
        .find(|version| version.id == game_version)
        .ok_or_else(|| {
            crate::ErrorKind::OtherError(format!(
                "Unknown Minecraft version {game_version}"
            ))
        })?;
    let info: VersionInfo = fetch_json(
        Method::GET,
        &version.url,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;
    info.downloads
        .server
        .map(|server| server.url)
        .ok_or_else(|| {
            crate::ErrorKind::OtherError(format!(
                "Minecraft {game_version} has no server download"
            ))
            .into()
        })
}

fn readme(name: &str, game_version: &str, report: &ServerPackReport) -> String {
    format!(
        "{name} server (Minecraft {game_version})\n\
         Exported from Threadrinth.\n\n\
         1. Install Java (the version this Minecraft version needs).\n\
         2. Run start.sh (Linux/macOS) or start.bat (Windows) once. The server\n\
            stops and asks you to accept the Minecraft EULA: set eula=true in\n\
            eula.txt (https://aka.ms/MinecraftEULA), then start it again.\n\
         3. Memory: edit -Xmx4G in the start script (Forge/NeoForge: in\n\
            user_jvm_args.txt after the first start).\n\n\
         Mods included: {}\n",
        report.mods_included
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_counts_mods() {
        let report = ServerPackReport { mods_included: 3 };
        let text = readme("Pack", "1.21.1", &report);
        assert!(text.contains("Pack server (Minecraft 1.21.1)"));
        assert!(text.contains("Mods included: 3"));
    }
}
