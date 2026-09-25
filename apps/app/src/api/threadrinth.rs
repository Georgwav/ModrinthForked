//! Commands for Threadrinth's own features, kept apart from the upstream
//! plugins so Modrinth updates merge cleanly.

use crate::api::Result;
use std::path::PathBuf;
use tauri::Runtime;
use theseus::instance::{ServerPackReport, ServerPackSelection};
use theseus::world_transfer::WorldTransferMode;

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("threadrinth")
        .invoke_handler(tauri::generate_handler![
            transfer_world,
            server_pack_selection,
            export_server_pack,
        ])
        .build()
}

/// Copies or moves a singleplayer world to another instance. Returns the
/// world's folder name there.
#[tauri::command]
pub async fn transfer_world(
    from_instance: &str,
    world: &str,
    to_instance: &str,
    mode: WorldTransferMode,
) -> Result<String> {
    Ok(theseus::world_transfer::transfer_world(
        from_instance,
        world,
        to_instance,
        mode,
    )
    .await?)
}

/// What the server pack export screen selects when it opens.
#[tauri::command]
pub async fn server_pack_selection(
    instance_id: &str,
) -> Result<ServerPackSelection> {
    Ok(theseus::instance::server_pack_selection(instance_id).await?)
}

/// Exports the selected files of an instance as a ready-to-run server zip.
#[tauri::command]
pub async fn export_server_pack(
    instance_id: &str,
    export_location: PathBuf,
    included: Vec<String>,
    excluded: Vec<String>,
) -> Result<ServerPackReport> {
    Ok(theseus::instance::export_server_pack(
        instance_id,
        export_location,
        included,
        excluded,
    )
    .await?)
}
