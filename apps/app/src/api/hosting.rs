//! Commands for hosting servers built from instances (Threadrinth).

use crate::api::Result;
use std::collections::HashMap;
use tauri::Runtime;
use theseus::hosting::{
    ConsoleLine, CreateServer, EditServer, HostedServer, PlayitLink,
    ServerStatus,
};

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("hosting")
        .invoke_handler(tauri::generate_handler![
            hosting_list,
            hosting_get,
            hosting_create,
            hosting_edit,
            hosting_delete,
            hosting_start,
            hosting_stop,
            hosting_kill,
            hosting_command,
            hosting_console,
            hosting_status,
            hosting_check_reachability,
            hosting_properties,
            hosting_set_properties,
            hosting_sync_mods,
            hosting_folder,
            playit_link_status,
            playit_start_link,
            playit_unlink,
        ])
        .build()
}

#[tauri::command]
pub async fn hosting_list() -> Result<Vec<HostedServer>> {
    Ok(theseus::hosting::list_servers().await?)
}

#[tauri::command]
pub async fn hosting_get(id: &str) -> Result<HostedServer> {
    Ok(theseus::hosting::get_server(id).await?)
}

#[tauri::command]
pub async fn hosting_create(request: CreateServer) -> Result<HostedServer> {
    Ok(theseus::hosting::create_server(request).await?)
}

#[tauri::command]
pub async fn hosting_edit(id: &str, edit: EditServer) -> Result<HostedServer> {
    Ok(theseus::hosting::edit_server(id, edit).await?)
}

#[tauri::command]
pub async fn hosting_delete(id: &str) -> Result<()> {
    Ok(theseus::hosting::delete_server(id).await?)
}

#[tauri::command]
pub async fn hosting_start(id: &str) -> Result<()> {
    Ok(theseus::hosting::start_server(id).await?)
}

#[tauri::command]
pub async fn hosting_stop(id: &str) -> Result<()> {
    Ok(theseus::hosting::stop_server(id).await?)
}

#[tauri::command]
pub async fn hosting_kill(id: &str) -> Result<()> {
    Ok(theseus::hosting::kill_server(id).await?)
}

#[tauri::command]
pub async fn hosting_command(id: &str, command: &str) -> Result<()> {
    Ok(theseus::hosting::send_command(id, command).await?)
}

#[tauri::command]
pub async fn hosting_console(
    id: &str,
    after: Option<u64>,
) -> Result<Vec<ConsoleLine>> {
    Ok(theseus::hosting::server_console(id, after))
}

#[tauri::command]
pub async fn hosting_status(id: &str) -> Result<ServerStatus> {
    Ok(theseus::hosting::server_status(id))
}

/// Checks again whether players outside this network can join.
#[tauri::command]
pub async fn hosting_check_reachability(id: &str) -> Result<()> {
    theseus::hosting::check_server_reachability(id).await;
    Ok(())
}

#[tauri::command]
pub async fn hosting_properties(id: &str) -> Result<Vec<(String, String)>> {
    Ok(theseus::hosting::server_properties(id).await?)
}

#[tauri::command]
pub async fn hosting_set_properties(
    id: &str,
    values: HashMap<String, String>,
) -> Result<()> {
    Ok(theseus::hosting::set_server_properties(id, values).await?)
}

#[tauri::command]
pub async fn hosting_sync_mods(id: &str) -> Result<usize> {
    Ok(theseus::hosting::sync_server_mods(id).await?)
}

/// The server's folder, to open in the file manager.
#[tauri::command]
pub async fn hosting_folder(id: &str) -> Result<String> {
    Ok(theseus::hosting::server_dir(id)
        .await?
        .to_string_lossy()
        .into_owned())
}

#[tauri::command]
pub async fn playit_link_status() -> Result<PlayitLink> {
    Ok(theseus::hosting::playit_link_status().await)
}

#[tauri::command]
pub async fn playit_start_link() -> Result<String> {
    Ok(theseus::hosting::start_playit_link().await?)
}

#[tauri::command]
pub async fn playit_unlink() -> Result<()> {
    Ok(theseus::hosting::unlink_playit().await?)
}
