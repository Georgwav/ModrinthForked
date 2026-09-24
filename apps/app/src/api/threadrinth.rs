//! Commands for Threadrinth's own features, kept apart from the upstream
//! plugins so Modrinth updates merge cleanly.

use crate::api::Result;
use std::path::PathBuf;
use tauri::Runtime;
use theseus::instance::ServerPackReport;
use theseus::world_transfer::WorldTransferMode;

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("threadrinth")
        .invoke_handler(tauri::generate_handler![
            transfer_world,
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

/// Exports an instance as a ready-to-run server zip.
#[tauri::command]
pub async fn export_server_pack(
    instance_id: &str,
    export_location: PathBuf,
) -> Result<ServerPackReport> {
    Ok(
        theseus::instance::export_server_pack(instance_id, export_location)
            .await?,
    )
}
