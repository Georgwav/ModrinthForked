//! Feed the Beast modpacks, from the public modpacks.ch API (no key needed),
//! installed as new instances like CurseForge modpacks.

use crate::State;
use crate::api::curseforge::bundle::{
    Bundle, BundleFile, ManualDownload, PackInstallReport, safe_relative_path,
};
use crate::api::curseforge::client;
use crate::state::ModLoader;
use crate::util::fetch::fetch_json;
use futures::StreamExt;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

const API_URL: &str = "https://api.modpacks.ch/public";
const WEBSITE_URL: &str = "https://www.feed-the-beast.com/modpacks";

/// A list of pack ids, from the popular list or a search.
#[derive(Deserialize, Debug, Default)]
#[serde(default)]
pub(crate) struct PackList {
    pub packs: Vec<u32>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub(crate) struct ApiPack {
    pub id: u32,
    pub name: String,
    pub synopsis: String,
    pub art: Vec<ApiArt>,
    pub authors: Vec<ApiAuthor>,
    pub installs: u64,
    pub plays: u64,
    pub updated: i64,
    pub tags: Vec<ApiTag>,
    pub versions: Vec<ApiVersionSummary>,
    pub status: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub(crate) struct ApiArt {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub(crate) struct ApiAuthor {
    pub name: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub(crate) struct ApiTag {
    pub name: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub(crate) struct ApiVersionSummary {
    pub id: u32,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub updated: i64,
    pub targets: Vec<ApiTarget>,
    pub private: bool,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub(crate) struct ApiTarget {
    pub name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub(crate) struct ApiVersion {
    pub id: u32,
    pub name: String,
    pub targets: Vec<ApiTarget>,
    pub files: Vec<ApiFile>,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub(crate) struct ApiFile {
    pub path: String,
    pub name: String,
    pub url: String,
    pub mirrors: Vec<String>,
    pub sha1: String,
    pub size: u64,
    pub clientonly: bool,
    pub serveronly: bool,
    pub curseforge: Option<ApiCurseForgeRef>,
}

#[derive(Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ApiCurseForgeRef {
    #[serde(deserialize_with = "number_or_string")]
    pub project: u32,
    #[serde(deserialize_with = "number_or_string")]
    pub file: u32,
}

fn number_or_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<u32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Number(u32),
        Text(String),
    }
    match Value::deserialize(deserializer)? {
        Value::Number(number) => Ok(number),
        Value::Text(text) => {
            text.trim().parse().map_err(serde::de::Error::custom)
        }
    }
}

/// A pack as the app shows it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FtbPack {
    pub id: u32,
    pub name: String,
    pub summary: String,
    pub icon_url: Option<String>,
    pub authors: Vec<String>,
    pub installs: u64,
    pub plays: u64,
    /// Unix seconds.
    pub updated: i64,
    pub tags: Vec<String>,
    pub website_url: String,
    /// Newest first.
    pub versions: Vec<FtbVersion>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FtbVersion {
    pub id: u32,
    pub name: String,
    /// `release`, `beta` or `alpha`.
    pub release_type: String,
    pub updated: i64,
    pub game_version: Option<String>,
    pub loader: Option<String>,
    pub loader_version: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct FtbSearchQuery {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub game_version: Option<String>,
    #[serde(default)]
    pub loader: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

fn target<'a>(targets: &'a [ApiTarget], kind: &str) -> Option<&'a ApiTarget> {
    targets.iter().find(|x| x.kind.eq_ignore_ascii_case(kind))
}

fn slug(name: &str) -> String {
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub(crate) fn pack_from_api(pack: ApiPack) -> FtbPack {
    let mut versions = pack
        .versions
        .iter()
        .filter(|x| !x.private)
        .map(|version| {
            let loader = target(&version.targets, "modloader");
            FtbVersion {
                id: version.id,
                name: version.name.clone(),
                release_type: version.kind.to_ascii_lowercase(),
                updated: version.updated,
                game_version: target(&version.targets, "game")
                    .map(|x| x.version.clone()),
                loader: loader.map(|x| x.name.to_ascii_lowercase()),
                loader_version: loader.map(|x| x.version.clone()),
            }
        })
        .collect::<Vec<_>>();
    versions.sort_by(|a, b| b.updated.cmp(&a.updated).then(b.id.cmp(&a.id)));
    let icon_url = pack
        .art
        .iter()
        .find(|x| x.kind == "square")
        .or_else(|| pack.art.iter().find(|x| x.kind == "logo"))
        .map(|x| x.url.clone())
        .filter(|x| !x.is_empty());
    FtbPack {
        id: pack.id,
        website_url: format!("{WEBSITE_URL}/{}-{}", pack.id, slug(&pack.name)),
        name: pack.name,
        summary: pack.synopsis,
        icon_url,
        authors: pack.authors.into_iter().map(|x| x.name).collect(),
        installs: pack.installs,
        plays: pack.plays,
        updated: pack.updated,
        tags: pack.tags.into_iter().map(|x| x.name).collect(),
        versions,
    }
}

async fn get<T: DeserializeOwned>(path: &str) -> crate::Result<T> {
    let state = State::get().await?;
    fetch_json(
        Method::GET,
        &format!("{API_URL}{path}"),
        None,
        None,
        None,
        &state.api_semaphore,
        &state.pool,
    )
    .await
}

pub async fn get_pack(id: u32) -> crate::Result<FtbPack> {
    Ok(pack_from_api(get_api_pack(id).await?))
}

async fn get_api_pack(id: u32) -> crate::Result<ApiPack> {
    let pack: ApiPack = get(&format!("/modpack/{id}")).await?;
    if pack.status != "success" {
        return Err(crate::ErrorKind::InputError(format!(
            "Feed the Beast has no modpack {id}"
        ))
        .into());
    }
    Ok(pack)
}

/// Whether any version of the pack is for the Minecraft version and loader.
pub(crate) fn pack_matches(
    pack: &FtbPack,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> bool {
    let game_version = game_version.filter(|x| !x.is_empty());
    let loader = loader.filter(|x| !x.is_empty());
    pack.versions.iter().any(|version| {
        game_version.is_none_or(|x| version.game_version.as_deref() == Some(x))
            && loader.is_none_or(|x| {
                version
                    .loader
                    .as_deref()
                    .is_some_and(|loader| loader.eq_ignore_ascii_case(x))
            })
    })
}

/// Popular packs, or the ones matching the search text.
pub async fn search(query: FtbSearchQuery) -> crate::Result<Vec<FtbPack>> {
    let limit = query.limit.unwrap_or(30).clamp(1, 100);
    let text = query.query.as_deref().map(str::trim).unwrap_or_default();
    let list: PackList = if text.is_empty() {
        get(&format!("/modpack/popular/installs/{limit}")).await?
    } else {
        // An empty search answers with an error status and no packs.
        get(&format!(
            "/modpack/search/{limit}?term={}",
            urlencoding::encode(text)
        ))
        .await?
    };
    let packs = futures::stream::iter(list.packs)
        .map(|id| async move { get_pack(id).await })
        .buffered(8)
        .collect::<Vec<_>>()
        .await;
    Ok(packs
        .into_iter()
        .filter_map(|pack| match pack {
            Ok(pack) => Some(pack),
            Err(error) => {
                tracing::warn!("Skipping a Feed the Beast pack: {error}");
                None
            }
        })
        .filter(|pack| !pack.versions.is_empty())
        .filter(|pack| {
            pack_matches(
                pack,
                query.game_version.as_deref(),
                query.loader.as_deref(),
            )
        })
        .collect())
}

fn loader_from_name(name: &str) -> ModLoader {
    ModLoader::from_string(&name.to_ascii_lowercase())
}

/// Where a file goes in the instance, or `None` if it leaves the instance.
pub(crate) fn file_path(file: &ApiFile) -> Option<String> {
    let name = crate::api::curseforge::bundle::safe_file_name(&file.name)?;
    safe_relative_path(&format!("{}/{name}", file.path))
}

/// Pack files split into downloads, files only on CurseForge (to resolve
/// there) and files that can't be placed.
pub(crate) fn split_files(
    files: &[ApiFile],
) -> (Vec<BundleFile>, Vec<(String, ApiCurseForgeRef)>) {
    let mut downloads = Vec::new();
    let mut curseforge = Vec::new();
    // Hidden files like `mods/.gitkeep` only keep empty folders in git.
    for file in files
        .iter()
        .filter(|x| !x.serveronly && !x.name.starts_with('.'))
    {
        let Some(path) = file_path(file) else {
            tracing::warn!(
                "Skipping unsafe pack file {}/{}",
                file.path,
                file.name
            );
            continue;
        };
        if !file.url.is_empty() {
            let mut urls = vec![file.url.clone()];
            urls.extend(file.mirrors.iter().filter(|x| !x.is_empty()).cloned());
            downloads.push(BundleFile {
                path,
                urls,
                sha1: Some(file.sha1.to_ascii_lowercase())
                    .filter(|x| x.len() == 40),
                size: file.size,
            });
        } else if let Some(reference) = file.curseforge {
            curseforge.push((path, reference));
        } else {
            tracing::warn!("Pack file {path} has no download");
        }
    }
    (downloads, curseforge)
}

fn folder_of(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(folder, _)| folder.to_string())
        .unwrap_or_default()
}

fn file_name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// Files only on CurseForge: through its API when this build has a key,
/// else as manual downloads.
async fn resolve_curseforge_files(
    references: Vec<(String, ApiCurseForgeRef)>,
) -> crate::Result<(Vec<BundleFile>, Vec<ManualDownload>)> {
    if references.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let manual_without_api = |references: &[(String, ApiCurseForgeRef)]| {
        references
            .iter()
            .map(|(path, reference)| ManualDownload {
                name: file_name_of(path),
                url: client::project_page_url(reference.project),
                folder: folder_of(path),
            })
            .collect::<Vec<_>>()
    };
    if !crate::api::curseforge::is_available() {
        return Ok((Vec::new(), manual_without_api(&references)));
    }
    let file_ids = references.iter().map(|(_, x)| x.file).collect::<Vec<_>>();
    let mut project_ids = references
        .iter()
        .map(|(_, x)| x.project)
        .collect::<Vec<_>>();
    project_ids.sort_unstable();
    project_ids.dedup();
    let files = client::get_files(&file_ids)
        .await?
        .into_iter()
        .map(|file| (file.id, file))
        .collect::<HashMap<_, _>>();
    let projects = client::get_mods(&project_ids)
        .await?
        .into_iter()
        .map(|project| (project.id, project))
        .collect::<HashMap<_, _>>();

    let mut downloads = Vec::new();
    let mut manual = Vec::new();
    for (path, reference) in references {
        let project = projects.get(&reference.project);
        let file = files.get(&reference.file);
        let downloadable = file.is_some_and(|file| {
            crate::api::curseforge::is_downloadable(
                file,
                project.and_then(|x| x.allow_mod_distribution),
            )
        });
        match file {
            Some(file) if downloadable => downloads.push(BundleFile {
                path,
                urls: file.download_url.iter().cloned().collect(),
                sha1: file.sha1().map(str::to_string),
                size: file.file_length,
            }),
            _ => manual.push(ManualDownload {
                name: file_name_of(&path),
                url: client::file_page_url(
                    project
                        .and_then(|x| x.links.as_ref())
                        .and_then(|x| x.website_url.as_deref()),
                    reference.project,
                    reference.file,
                ),
                folder: folder_of(&path),
            }),
        }
    }
    Ok((downloads, manual))
}

/// The pack version as a bundle to install.
pub(crate) fn bundle_from_version(
    pack: &ApiPack,
    version: &ApiVersion,
    files: Vec<BundleFile>,
) -> crate::Result<Bundle> {
    let game_version = target(&version.targets, "game")
        .map(|x| x.version.clone())
        .ok_or_else(|| {
            crate::ErrorKind::InputError(format!(
                "{} {} doesn't say which Minecraft version it is for",
                pack.name, version.name
            ))
        })?;
    let loader = target(&version.targets, "modloader");
    Ok(Bundle {
        name: pack.name.clone(),
        version: version.name.clone(),
        summary: Some(pack.synopsis.clone()).filter(|x| !x.is_empty()),
        game_version,
        loader: loader
            .map(|x| loader_from_name(&x.name))
            .unwrap_or(ModLoader::Vanilla),
        loader_version: loader.map(|x| x.version.clone()),
        files,
        overrides: None,
    })
}

/// Starts installing a Feed the Beast pack version as a new instance.
pub async fn install_pack(
    pack_id: u32,
    version_id: u32,
) -> crate::Result<PackInstallReport> {
    let (bundle, icon_url, manual) =
        prepare_install(pack_id, version_id).await?;
    crate::api::curseforge::bundle::install(bundle, icon_url, manual).await
}

pub(crate) async fn prepare_install(
    pack_id: u32,
    version_id: u32,
) -> crate::Result<(Bundle, Option<String>, Vec<ManualDownload>)> {
    let pack = get_api_pack(pack_id).await?;
    let version: ApiVersion =
        get(&format!("/modpack/{pack_id}/{version_id}")).await?;
    if version.status != "success" {
        return Err(crate::ErrorKind::InputError(
            version.message.unwrap_or_else(|| {
                format!(
                    "Feed the Beast has no version {version_id} of {pack_id}"
                )
            }),
        )
        .into());
    }
    let (mut downloads, curseforge) = split_files(&version.files);
    let (resolved, manual) = resolve_curseforge_files(curseforge).await?;
    downloads.extend(resolved);
    let bundle = bundle_from_version(&pack, &version, downloads)?;
    let icon_url = pack_from_api(pack).icon_url;
    Ok((bundle, icon_url, manual))
}
