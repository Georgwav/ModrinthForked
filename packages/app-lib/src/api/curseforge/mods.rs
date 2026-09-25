//! Searching CurseForge and installing its mods into instances.

use super::bundle::ManualDownload;
use super::client::{
    self, CLASS_DATA_PACKS, CLASS_MODPACKS, CLASS_MODS, CLASS_RESOURCE_PACKS,
    CLASS_SHADERS, GAME_ID, PagedResponse, RELATION_REQUIRED, mod_loader_name,
    mod_loader_type,
};
use crate::State;
use crate::event::InstancePayloadType;
use crate::event::emit::emit_instance;
use crate::state::{ContentSourceKind, ModLoader, ProjectType};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// How many levels of required dependencies an install follows.
const MAX_DEPENDENCY_DEPTH: usize = 3;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CurseForgeClass {
    Mod,
    Modpack,
    ResourcePack,
    DataPack,
    Shader,
}

impl CurseForgeClass {
    fn id(self) -> u32 {
        match self {
            Self::Mod => CLASS_MODS,
            Self::Modpack => CLASS_MODPACKS,
            Self::ResourcePack => CLASS_RESOURCE_PACKS,
            Self::DataPack => CLASS_DATA_PACKS,
            Self::Shader => CLASS_SHADERS,
        }
    }

    fn from_id(id: Option<u32>) -> Self {
        match id {
            Some(CLASS_MODPACKS) => Self::Modpack,
            Some(CLASS_RESOURCE_PACKS) => Self::ResourcePack,
            Some(CLASS_DATA_PACKS) => Self::DataPack,
            Some(CLASS_SHADERS) => Self::Shader,
            _ => Self::Mod,
        }
    }

    /// Where it goes in an instance, for content installed into one.
    fn project_type(self) -> ProjectType {
        match self {
            Self::ResourcePack => ProjectType::ResourcePack,
            Self::DataPack => ProjectType::DataPack,
            Self::Shader => ProjectType::ShaderPack,
            Self::Mod | Self::Modpack => ProjectType::Mod,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CurseForgeSort {
    Popularity,
    Updated,
    Downloads,
    Name,
    Newest,
    Rating,
}

impl CurseForgeSort {
    /// The API's `ModsSearchSortField` and whether it sorts descending.
    fn field(self) -> (u32, &'static str) {
        match self {
            Self::Popularity => (2, "desc"),
            Self::Updated => (3, "desc"),
            Self::Name => (4, "asc"),
            Self::Downloads => (6, "desc"),
            Self::Newest => (11, "desc"),
            Self::Rating => (12, "desc"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CurseForgeSearchQuery {
    pub class: CurseForgeClass,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub game_version: Option<String>,
    /// `forge`, `fabric`, `quilt` or `neoforge`.
    #[serde(default)]
    pub loader: Option<String>,
    pub sort: CurseForgeSort,
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub page_size: Option<u32>,
}

/// A project as the app shows it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CurseForgeProject {
    pub id: u32,
    pub name: String,
    pub slug: String,
    pub summary: String,
    pub authors: Vec<String>,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub website_url: Option<String>,
    pub updated: Option<String>,
    pub created: Option<String>,
    /// `false` when the author doesn't allow downloads through other apps.
    pub allow_distribution: bool,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub categories: Vec<String>,
    pub class: CurseForgeClass,
    /// Likes on CurseForge.
    pub thumbs_up: u64,
    pub gallery: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CurseForgeSearchResults {
    pub projects: Vec<CurseForgeProject>,
    pub index: u32,
    pub page_size: u32,
    pub total: u64,
}

/// A version (file) of a project.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CurseForgeFile {
    pub id: u32,
    pub project_id: u32,
    pub name: String,
    pub file_name: String,
    /// `release`, `beta` or `alpha`.
    pub release_type: String,
    pub date: String,
    pub size: u64,
    /// Minecraft versions it is for.
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    /// `false` when it has to be downloaded from the website.
    pub downloadable: bool,
    /// Its page on the website.
    pub page_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CurseForgeFiles {
    pub files: Vec<CurseForgeFile>,
    pub index: u32,
    pub total: u64,
}

/// What installing a mod did.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct CurseForgeModInstall {
    /// Paths of the installed files in the instance.
    pub installed: Vec<String>,
    /// Files (the mod or its dependencies) to download by hand.
    pub manual_downloads: Vec<ManualDownload>,
    /// Required dependencies with no file for this instance.
    pub missing_dependencies: Vec<String>,
}

const LOADER_NAMES: &[&str] = &["forge", "fabric", "quilt", "neoforge"];

fn is_minecraft_version(tag: &str) -> bool {
    tag.chars().next().is_some_and(|c| c.is_ascii_digit())
        || tag.to_ascii_lowercase().starts_with("snapshot")
}

pub(crate) fn project_from_api(project: client::Mod) -> CurseForgeProject {
    let mut game_versions = Vec::new();
    let mut loaders = Vec::new();
    for index in &project.latest_files_indexes {
        if !game_versions.contains(&index.game_version) {
            game_versions.push(index.game_version.clone());
        }
        if let Some(loader) = index.mod_loader.and_then(mod_loader_name)
            && !loaders.iter().any(|x| x == loader)
        {
            loaders.push(loader.to_string());
        }
    }
    CurseForgeProject {
        id: project.id,
        name: project.name,
        slug: project.slug,
        summary: project.summary,
        authors: project.authors.into_iter().map(|x| x.name).collect(),
        downloads: project.download_count.max(0.0) as u64,
        icon_url: project
            .logo
            .and_then(|logo| logo.thumbnail_url.or(logo.url))
            .filter(|url| !url.is_empty()),
        website_url: project.links.and_then(|links| links.website_url),
        updated: project.date_modified,
        created: project.date_created,
        allow_distribution: project.allow_mod_distribution != Some(false),
        game_versions,
        loaders,
        categories: project.categories.into_iter().map(|x| x.name).collect(),
        class: CurseForgeClass::from_id(project.class_id),
        thumbs_up: project.thumbs_up_count,
        gallery: project
            .screenshots
            .into_iter()
            .filter_map(|x| x.url.or(x.thumbnail_url))
            .filter(|url| !url.is_empty())
            .collect(),
    }
}

fn release_type_name(release_type: u32) -> &'static str {
    match release_type {
        2 => "beta",
        3 => "alpha",
        _ => "release",
    }
}

/// Whether a file can be downloaded through the API.
pub(crate) fn is_downloadable(
    file: &client::File,
    allow_distribution: Option<bool>,
) -> bool {
    allow_distribution != Some(false)
        && file
            .download_url
            .as_deref()
            .is_some_and(|url| !url.is_empty())
}

pub(crate) fn file_from_api(
    file: client::File,
    website_url: Option<&str>,
    allow_distribution: Option<bool>,
) -> CurseForgeFile {
    let downloadable = is_downloadable(&file, allow_distribution);
    let (game_versions, loaders) = split_game_versions(&file.game_versions);
    CurseForgeFile {
        id: file.id,
        project_id: file.mod_id,
        page_url: client::file_page_url(website_url, file.mod_id, file.id),
        name: if file.display_name.is_empty() {
            file.file_name.clone()
        } else {
            file.display_name
        },
        file_name: file.file_name,
        release_type: release_type_name(file.release_type).to_string(),
        date: file.file_date,
        size: file.file_length,
        game_versions,
        loaders,
        downloadable,
    }
}

/// CurseForge lists Minecraft versions, loaders and sides in one list.
fn split_game_versions(tags: &[String]) -> (Vec<String>, Vec<String>) {
    let mut game_versions = Vec::new();
    let mut loaders = Vec::new();
    for tag in tags {
        let lower = tag.to_ascii_lowercase();
        if LOADER_NAMES.contains(&lower.as_str()) {
            loaders.push(lower);
        } else if is_minecraft_version(tag) {
            game_versions.push(tag.clone());
        }
    }
    (game_versions, loaders)
}

/// Whether a mod file works on an instance: made for its Minecraft version
/// and loader (Quilt also runs Fabric mods).
pub(crate) fn file_fits_instance(
    file: &client::File,
    game_version: &str,
    loader: ModLoader,
) -> bool {
    if file.is_server_pack == Some(true) || file.is_available == Some(false) {
        return false;
    }
    let (game_versions, loaders) = split_game_versions(&file.game_versions);
    if !game_versions.iter().any(|x| x == game_version) {
        return false;
    }
    match loader {
        ModLoader::Vanilla => false,
        ModLoader::Quilt => {
            loaders.iter().any(|x| x == "quilt" || x == "fabric")
                || loaders.is_empty()
        }
        loader => {
            loaders.iter().any(|x| x == loader.as_str()) || loaders.is_empty()
        }
    }
}

/// The newest file that works on the instance. Files made for the exact
/// loader win over Fabric ones on Quilt.
pub(crate) fn newest_fitting_file(
    files: Vec<client::File>,
    game_version: &str,
    loader: ModLoader,
) -> Option<client::File> {
    let exact = |file: &client::File| {
        file.game_versions
            .iter()
            .any(|x| x.eq_ignore_ascii_case(loader.as_str()))
    };
    let mut fitting = files
        .into_iter()
        .filter(|file| file_fits_instance(file, game_version, loader))
        .collect::<Vec<_>>();
    // RFC 3339 dates in UTC sort as text.
    fitting.sort_by(|a, b| {
        exact(b)
            .cmp(&exact(a))
            .then_with(|| b.file_date.cmp(&a.file_date))
    });
    fitting.into_iter().next()
}

/// The newest file for a Minecraft version, for content without a loader
/// (resource packs, data packs and shaders).
pub(crate) fn newest_file_for_version(
    files: Vec<client::File>,
    game_version: &str,
) -> Option<client::File> {
    files
        .into_iter()
        .filter(|file| {
            file.is_server_pack != Some(true)
                && file.is_available != Some(false)
                && split_game_versions(&file.game_versions)
                    .0
                    .iter()
                    .any(|x| x == game_version)
        })
        // RFC 3339 dates in UTC sort as text.
        .max_by(|a, b| a.file_date.cmp(&b.file_date))
}

pub(crate) fn search_path(query: &CurseForgeSearchQuery) -> String {
    let (sort_field, sort_order) = query.sort.field();
    let mut params = vec![
        format!("gameId={GAME_ID}"),
        format!("classId={}", query.class.id()),
        format!("sortField={sort_field}"),
        format!("sortOrder={sort_order}"),
        format!("index={}", query.index),
        format!("pageSize={}", query.page_size.unwrap_or(20).clamp(1, 50)),
    ];
    if let Some(text) = query.query.as_deref().map(str::trim)
        && !text.is_empty()
    {
        params.push(format!("searchFilter={}", urlencoding::encode(text)));
    }
    if let Some(version) = query.game_version.as_deref()
        && !version.is_empty()
    {
        params.push(format!("gameVersion={}", urlencoding::encode(version)));
    }
    if let Some(loader) = query.loader.as_deref().and_then(mod_loader_type) {
        params.push(format!("modLoaderType={loader}"));
    }
    format!("/v1/mods/search?{}", params.join("&"))
}

pub async fn search(
    query: CurseForgeSearchQuery,
) -> crate::Result<CurseForgeSearchResults> {
    let response: PagedResponse<client::Mod> =
        client::get(&search_path(&query)).await?;
    Ok(search_results_from_api(response, &query))
}

pub(crate) fn search_results_from_api(
    response: PagedResponse<client::Mod>,
    query: &CurseForgeSearchQuery,
) -> CurseForgeSearchResults {
    let pagination = response.pagination.unwrap_or_default();
    CurseForgeSearchResults {
        projects: response.data.into_iter().map(project_from_api).collect(),
        index: pagination.index.max(query.index),
        page_size: pagination.page_size,
        // The API pages no further than 10,000 results.
        total: pagination.total_count.min(10_000),
    }
}

pub async fn get_project(id: u32) -> crate::Result<CurseForgeProject> {
    Ok(project_from_api(client::get_mod(id).await?))
}

/// A project's description, as HTML from CurseForge (sanitized where it is
/// shown).
pub async fn get_description(id: u32) -> crate::Result<String> {
    client::get_description(id).await
}

pub(crate) fn files_path(
    project_id: u32,
    game_version: Option<&str>,
    loader: Option<&str>,
    index: u32,
) -> String {
    let mut params = vec![format!("index={index}"), "pageSize=50".to_string()];
    if let Some(version) = game_version.filter(|x| !x.is_empty()) {
        params.push(format!("gameVersion={}", urlencoding::encode(version)));
    }
    if let Some(loader) = loader.and_then(mod_loader_type) {
        params.push(format!("modLoaderType={loader}"));
    }
    format!("/v1/mods/{project_id}/files?{}", params.join("&"))
}

/// A project's files, newest first.
pub async fn get_files(
    project_id: u32,
    game_version: Option<String>,
    loader: Option<String>,
    index: u32,
) -> crate::Result<CurseForgeFiles> {
    let project = client::get_mod(project_id).await?;
    let response: PagedResponse<client::File> = client::get(&files_path(
        project_id,
        game_version.as_deref(),
        loader.as_deref(),
        index,
    ))
    .await?;
    let pagination = response.pagination.unwrap_or_default();
    let website_url =
        project.links.as_ref().and_then(|x| x.website_url.clone());
    let mut files = response
        .data
        .into_iter()
        .filter(|file| file.is_server_pack != Some(true))
        .map(|file| {
            file_from_api(
                file,
                website_url.as_deref(),
                project.allow_mod_distribution,
            )
        })
        .collect::<Vec<_>>();
    files.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(CurseForgeFiles {
        files,
        index,
        total: pagination.total_count,
    })
}

/// Files of a project that may fit an instance: the API filters by
/// Minecraft version and, except on Quilt (which also runs Fabric mods), by
/// loader.
async fn candidate_files(
    project_id: u32,
    game_version: &str,
    loader: Option<ModLoader>,
) -> crate::Result<Vec<client::File>> {
    let loader_filter = match loader {
        None | Some(ModLoader::Quilt | ModLoader::Vanilla) => None,
        Some(loader) => Some(loader.as_str()),
    };
    let response: PagedResponse<client::File> = client::get(&files_path(
        project_id,
        Some(game_version),
        loader_filter,
        0,
    ))
    .await?;
    Ok(response.data)
}

/// Installs a mod, resource pack, data pack or shader into an instance: the
/// given file, or else the newest one for the instance's Minecraft version
/// (and loader, for mods, with their required dependencies).
pub async fn install_mod(
    instance_id: &str,
    project_id: u32,
    file_id: Option<u32>,
) -> crate::Result<CurseForgeModInstall> {
    let state = State::get().await?;
    let metadata =
        crate::api::instance::get(instance_id)
            .await?
            .ok_or_else(|| {
                crate::ErrorKind::InputError("Unknown instance".to_string())
            })?;
    if metadata.quarantined {
        return Err(crate::ErrorKind::InputError(
            "Content in quarantined instances cannot be changed.".to_string(),
        )
        .into());
    }
    let game_version = metadata.applied_content_set.game_version.clone();
    let loader = metadata.applied_content_set.loader;
    let class =
        CurseForgeClass::from_id(client::get_mod(project_id).await?.class_id);
    if class == CurseForgeClass::Modpack {
        return Err(crate::ErrorKind::InputError(
            "Modpacks are installed as new instances.".to_string(),
        )
        .into());
    }
    if class == CurseForgeClass::Mod && loader == ModLoader::Vanilla {
        return Err(crate::ErrorKind::InputError(format!(
            "{} has no mod loader. Mods need an instance with Forge, NeoForge, Fabric or Quilt.",
            metadata.instance.name
        ))
        .into());
    }

    let existing = crate::state::instances::commands::list_project_files(
        instance_id,
        &state,
    )
    .await?
    .into_iter()
    .filter_map(|file| {
        file.relative_path
            .rsplit('/')
            .next()
            .map(|name| name.trim_end_matches(".disabled").to_ascii_lowercase())
    })
    .collect::<HashSet<_>>();

    let mut report = CurseForgeModInstall::default();
    let mut visited = HashSet::new();
    // (project, pinned file, depth)
    let mut queue = vec![(project_id, file_id, 0_usize)];
    let mut first = true;
    while let Some((project_id, file_id, depth)) = queue.pop() {
        if !visited.insert(project_id) {
            continue;
        }
        let project = client::get_mod(project_id).await?;
        let website_url =
            project.links.as_ref().and_then(|x| x.website_url.clone());
        let file = match file_id {
            Some(file_id) => {
                client::get_files(&[file_id]).await?.into_iter().next()
            }
            None if class == CurseForgeClass::Mod => newest_fitting_file(
                candidate_files(project_id, &game_version, Some(loader))
                    .await?,
                &game_version,
                loader,
            ),
            None => newest_file_for_version(
                candidate_files(project_id, &game_version, None).await?,
                &game_version,
            ),
        };
        let Some(file) = file else {
            if first {
                return Err(crate::ErrorKind::InputError(
                    if class == CurseForgeClass::Mod {
                        format!(
                            "{} has no version for Minecraft {game_version} with {}.",
                            project.name,
                            loader_display_name(loader)
                        )
                    } else {
                        format!(
                            "{} has no version for Minecraft {game_version}.",
                            project.name
                        )
                    },
                )
                .into());
            }
            report.missing_dependencies.push(project.name);
            continue;
        };
        first = false;

        if class == CurseForgeClass::Mod && depth < MAX_DEPENDENCY_DEPTH {
            for dependency in &file.dependencies {
                if dependency.relation_type == RELATION_REQUIRED {
                    queue.push((dependency.mod_id, None, depth + 1));
                }
            }
        }

        let Some(file_name) = super::bundle::safe_file_name(&file.file_name)
        else {
            continue;
        };
        if existing.contains(&file_name.to_ascii_lowercase()) && depth > 0 {
            // A dependency that is already there.
            continue;
        }
        if !is_downloadable(&file, project.allow_mod_distribution) {
            report.manual_downloads.push(ManualDownload {
                name: project.name.clone(),
                url: client::file_page_url(
                    website_url.as_deref(),
                    project_id,
                    file.id,
                ),
                folder: class.project_type().get_folder().to_string(),
            });
            continue;
        }
        let url = file.download_url.clone().unwrap_or_default();
        let bytes = crate::util::fetch::fetch(
            &url,
            file.sha1(),
            None,
            None,
            &state.fetch_semaphore,
            &state.pool,
        )
        .await?;
        let path = crate::state::instances::commands::add_project_bytes(
            instance_id,
            &file_name,
            bytes,
            file.sha1(),
            Some(class.project_type()),
            ContentSourceKind::Local,
            None,
            None,
            &state,
        )
        .await?;
        report.installed.push(path);
    }

    emit_instance(instance_id, InstancePayloadType::Edited).await?;
    Ok(report)
}

fn loader_display_name(loader: ModLoader) -> &'static str {
    match loader {
        ModLoader::Vanilla => "no mod loader",
        ModLoader::Forge => "Forge",
        ModLoader::NeoForge => "NeoForge",
        ModLoader::Fabric => "Fabric",
        ModLoader::Quilt => "Quilt",
    }
}
