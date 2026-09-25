//! Installing CurseForge modpacks as new instances.

use super::bundle::{
    Bundle, BundleFile, ManualDownload, PackInstallReport, ZipOverrides,
    parse_loader_id, safe_file_name,
};
use super::client::{
    self, CLASS_RESOURCE_PACKS, CLASS_SHADERS, file_page_url, project_page_url,
};
use super::mods::is_downloadable;
use crate::State;
use crate::state::ModLoader;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

/// The `manifest.json` of a CurseForge modpack zip.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    pub minecraft: ManifestMinecraft,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub files: Vec<ManifestFile>,
    #[serde(default)]
    pub overrides: Option<String>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManifestMinecraft {
    pub version: String,
    #[serde(default)]
    pub mod_loaders: Vec<ManifestModLoader>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub(crate) struct ManifestModLoader {
    pub id: String,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManifestFile {
    #[serde(rename = "projectID")]
    pub project_id: u32,
    #[serde(rename = "fileID")]
    pub file_id: u32,
    #[serde(default = "default_required")]
    pub required: bool,
}

fn default_required() -> bool {
    true
}

impl Manifest {
    /// The pack's loader: the primary one, else the first known one.
    pub fn loader(&self) -> Option<(ModLoader, String)> {
        let loaders = &self.minecraft.mod_loaders;
        loaders
            .iter()
            .filter(|x| x.primary)
            .chain(loaders.iter())
            .find_map(|x| parse_loader_id(&x.id))
    }
}

pub(crate) fn read_manifest(archive: &Path) -> crate::Result<Manifest> {
    let file = std::fs::File::open(archive)
        .map_err(|e| crate::util::io::IOError::with_path(e, archive))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(std::io::Error::from)?;
    let mut entry = zip.by_name("manifest.json").map_err(|_| {
        crate::ErrorKind::InputError(
            "This isn't a CurseForge modpack: it has no manifest.json"
                .to_string(),
        )
    })?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    Ok(serde_json::from_str(text.trim_start_matches('\u{feff}'))?)
}

/// The instance folder for a project's class.
pub(crate) fn class_folder(class_id: Option<u32>) -> &'static str {
    match class_id {
        Some(CLASS_RESOURCE_PACKS) => "resourcepacks",
        Some(CLASS_SHADERS) => "shaderpacks",
        _ => "mods",
    }
}

/// Splits the pack's files into downloads and files to get by hand.
pub(crate) fn resolve_files(
    manifest: &Manifest,
    files: &[client::File],
    projects: &[client::Mod],
) -> (Vec<BundleFile>, Vec<ManualDownload>) {
    let files = files
        .iter()
        .map(|file| (file.id, file))
        .collect::<HashMap<_, _>>();
    let projects = projects
        .iter()
        .map(|project| (project.id, project))
        .collect::<HashMap<_, _>>();
    let mut downloads = Vec::new();
    let mut manual = Vec::new();
    for entry in manifest.files.iter().filter(|x| x.required) {
        let project = projects.get(&entry.project_id);
        let folder = class_folder(project.and_then(|x| x.class_id));
        let website_url = project
            .and_then(|x| x.links.as_ref())
            .and_then(|x| x.website_url.as_deref());
        let project_name = project
            .map(|x| x.name.clone())
            .unwrap_or_else(|| format!("Project {}", entry.project_id));
        let Some(file) = files.get(&entry.file_id) else {
            manual.push(ManualDownload {
                name: project_name,
                url: file_page_url(
                    website_url,
                    entry.project_id,
                    entry.file_id,
                ),
                folder: folder.to_string(),
            });
            continue;
        };
        let Some(file_name) = safe_file_name(&file.file_name) else {
            continue;
        };
        if !is_downloadable(
            file,
            project.and_then(|x| x.allow_mod_distribution),
        ) {
            manual.push(ManualDownload {
                name: if file.display_name.is_empty() {
                    file_name
                } else {
                    file.display_name.clone()
                },
                url: file_page_url(website_url, entry.project_id, file.id),
                folder: folder.to_string(),
            });
            continue;
        }
        downloads.push(BundleFile {
            path: format!("{folder}/{file_name}"),
            urls: file.download_url.iter().cloned().collect(),
            sha1: file.sha1().map(str::to_string),
            size: file.file_length,
        });
    }
    (downloads, manual)
}

/// Starts installing a CurseForge modpack as a new instance: the given file,
/// or else the project's main file.
pub async fn install_modpack(
    project_id: u32,
    file_id: Option<u32>,
) -> crate::Result<PackInstallReport> {
    let state = State::get().await?;
    let project = client::get_mod(project_id).await?;
    let file_id = file_id.unwrap_or(project.main_file_id);
    let pack_file = client::get_files(&[file_id])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| {
            crate::ErrorKind::InputError(format!(
                "CurseForge has no file {file_id}"
            ))
        })?;
    let url = pack_file
        .download_url
        .clone()
        .filter(|_| is_downloadable(&pack_file, project.allow_mod_distribution))
        .ok_or_else(|| {
            crate::ErrorKind::InputError(format!(
                "The author of {} doesn't allow downloading it in other apps. Download it from {} and add it with \"Import from file\".",
                project.name,
                project
                    .links
                    .as_ref()
                    .and_then(|x| x.website_url.clone())
                    .unwrap_or_else(|| project_page_url(project_id))
            ))
        })?;

    let archive = super::bundle::fetch_pack_content_file(
        &state,
        &[&url],
        None,
        pack_file.sha1(),
        None,
        None,
        None,
    )
    .await?;
    let archive_path = archive.path().to_path_buf();
    let manifest = {
        let path = archive_path.clone();
        tokio::task::spawn_blocking(move || read_manifest(&path))
            .await
            .map_err(|e| crate::ErrorKind::OtherError(e.to_string()))??
    };

    let file_ids = manifest
        .files
        .iter()
        .filter(|x| x.required)
        .map(|x| x.file_id)
        .collect::<Vec<_>>();
    let mut project_ids = manifest
        .files
        .iter()
        .map(|x| x.project_id)
        .collect::<Vec<_>>();
    project_ids.sort_unstable();
    project_ids.dedup();
    let files = client::get_files(&file_ids).await?;
    let projects = client::get_mods(&project_ids).await?;
    let (downloads, manual) = resolve_files(&manifest, &files, &projects);
    let (loader, loader_version) = manifest
        .loader()
        .map(|(loader, version)| (loader, Some(version)))
        .unwrap_or((ModLoader::Vanilla, None));

    let bundle = Bundle {
        name: manifest
            .name
            .clone()
            .filter(|x| !x.trim().is_empty())
            .unwrap_or_else(|| project.name.clone()),
        version: manifest
            .version
            .clone()
            .unwrap_or_else(|| pack_file.display_name.clone()),
        summary: Some(project.summary.clone()).filter(|x| !x.is_empty()),
        game_version: manifest.minecraft.version.clone(),
        loader,
        loader_version,
        files: downloads,
        overrides: Some(ZipOverrides {
            archive: archive_path,
            prefix: manifest
                .overrides
                .clone()
                .unwrap_or_else(|| "overrides".to_string()),
        }),
    };
    let icon_url = project.logo.as_ref().and_then(|logo| {
        logo.thumbnail_url.clone().or_else(|| logo.url.clone())
    });
    let report = super::bundle::install(bundle, icon_url, manual).await;
    // The archive stays in the content store until the bundle is built.
    drop(archive);
    report
}
