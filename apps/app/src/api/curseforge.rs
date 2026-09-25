//! Commands for Threadrinth's CurseForge tab: CurseForge mods and modpacks
//! and Feed the Beast modpacks. Kept apart from the upstream plugins so
//! Modrinth updates merge cleanly.

use crate::api::Result;
use serde::Serialize;
use tauri::Runtime;
use theseus::curseforge::{
    CurseForgeFiles, CurseForgeModInstall, CurseForgeProject,
    CurseForgeSearchQuery, CurseForgeSearchResults, PackInstallReport,
};
use theseus::ftb::{FtbPack, FtbSearchQuery};

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("curseforge")
        .invoke_handler(tauri::generate_handler![
            curseforge_status,
            curseforge_search,
            curseforge_project,
            curseforge_files,
            curseforge_install_mod,
            curseforge_install_modpack,
            ftb_search,
            ftb_pack,
            ftb_install_pack,
        ])
        .build()
}

#[derive(Serialize)]
pub struct CurseForgeStatus {
    /// Whether this build has a CurseForge API key.
    pub available: bool,
}

#[tauri::command]
pub fn curseforge_status() -> CurseForgeStatus {
    CurseForgeStatus {
        available: theseus::curseforge::is_available(),
    }
}

#[tauri::command]
pub async fn curseforge_search(
    query: CurseForgeSearchQuery,
) -> Result<CurseForgeSearchResults> {
    Ok(theseus::curseforge::search(query).await?)
}

#[tauri::command]
pub async fn curseforge_project(project_id: u32) -> Result<CurseForgeProject> {
    Ok(theseus::curseforge::get_project(project_id).await?)
}

#[tauri::command]
pub async fn curseforge_files(
    project_id: u32,
    game_version: Option<String>,
    loader: Option<String>,
    index: Option<u32>,
) -> Result<CurseForgeFiles> {
    Ok(theseus::curseforge::get_files(
        project_id,
        game_version,
        loader,
        index.unwrap_or(0),
    )
    .await?)
}

/// Installs a mod (the given file, or the newest one that fits) and its
/// required dependencies into an instance.
#[tauri::command]
pub async fn curseforge_install_mod(
    instance_id: &str,
    project_id: u32,
    file_id: Option<u32>,
) -> Result<CurseForgeModInstall> {
    Ok(
        theseus::curseforge::install_mod(instance_id, project_id, file_id)
            .await?,
    )
}

/// Starts installing a CurseForge modpack as a new instance.
#[tauri::command]
pub async fn curseforge_install_modpack(
    project_id: u32,
    file_id: Option<u32>,
) -> Result<PackInstallReport> {
    Ok(theseus::curseforge::install_modpack(project_id, file_id).await?)
}

#[tauri::command]
pub async fn ftb_search(query: FtbSearchQuery) -> Result<Vec<FtbPack>> {
    Ok(theseus::ftb::search(query).await?)
}

#[tauri::command]
pub async fn ftb_pack(pack_id: u32) -> Result<FtbPack> {
    Ok(theseus::ftb::get_pack(pack_id).await?)
}

/// Starts installing a Feed the Beast pack version as a new instance.
#[tauri::command]
pub async fn ftb_install_pack(
    pack_id: u32,
    version_id: u32,
) -> Result<PackInstallReport> {
    Ok(theseus::ftb::install_pack(pack_id, version_id).await?)
}
