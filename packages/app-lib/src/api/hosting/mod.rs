//! Hosting: Minecraft servers built from an instance and run on this
//! computer. Each server is a folder in `servers/` next to the instances
//! folder, described by `threadrinth-server.json`, so it travels with the app
//! folder like instances do.

mod process;
mod properties;
mod public;
mod setup;

pub use process::stop_all_servers;
pub use process::{
    ConsoleLine, ConsoleStream, ServerState, ServerStatus, kill_server,
    send_command, server_console, server_status, start_server, stop_server,
};
pub use properties::{server_properties, set_server_properties};
pub use public::{
    PlayitLink, PublicAddress, PublicVia, playit_link_status,
    start_playit_link, unlink_playit,
};

use crate::api::instance::{
    ServerPackSelection, collect_server_files, write_dir,
};
use crate::state::{ModLoader, State};
use crate::util::io::{self, IOError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The folder next to the instances folder that holds the servers.
pub const SERVERS_FOLDER: &str = "servers";
const SERVER_FILE: &str = "threadrinth-server.json";
const DEFAULT_PORT: u16 = 25565;
const DEFAULT_MEMORY_MB: u32 = 4096;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HostedServer {
    /// The server's folder name in `servers/`.
    pub id: String,
    pub name: String,
    /// The instance it was built from, used to update its mods.
    pub instance_id: Option<String>,
    pub game_version: String,
    pub loader: ModLoader,
    pub loader_version: Option<String>,
    pub memory_mb: u32,
    pub port: u16,
    /// The user accepted the Minecraft EULA for this server.
    pub eula_accepted: bool,
    pub launch: LaunchTarget,
    /// The files picked from the instance, reused when updating the mods.
    pub selection: Option<ServerPackSelection>,
    #[serde(default)]
    pub public_access: PublicAccess,
    pub created: DateTime<Utc>,
}

/// How the server is started.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LaunchTarget {
    /// `java -jar <path> nogui`
    Jar { path: String },
    /// `java @<path> nogui`, the arguments file modern Forge and NeoForge
    /// installers write (`unix_args.txt`; `win_args.txt` next to it is used
    /// on Windows).
    ArgsFile { path: String },
}

/// How players outside this network reach the server.
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum PublicAccess {
    /// Only this network (and port forwarding set up by hand).
    #[default]
    Off,
    /// Try the router's automatic port forwarding (UPnP), then playit.gg.
    Auto,
}

/// Where the server's world comes from.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorldSource {
    /// A new world, generated on first start.
    New { seed: Option<String> },
    /// A copy of a singleplayer world from an instance.
    Copy { instance_id: String, world: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateServer {
    pub instance_id: String,
    pub name: String,
    pub included: Vec<String>,
    pub excluded: Vec<String>,
    pub world: WorldSource,
    pub memory_mb: Option<u32>,
    pub eula_accepted: bool,
}

/// Settings changed from the server page.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EditServer {
    pub name: Option<String>,
    pub memory_mb: Option<u32>,
    pub port: Option<u16>,
    pub eula_accepted: Option<bool>,
    pub public_access: Option<PublicAccess>,
}

pub(crate) async fn servers_dir() -> crate::Result<PathBuf> {
    let state = State::get().await?;
    let instances = state.directories.instances_dir();
    Ok(instances.parent().map_or_else(
        || instances.join(SERVERS_FOLDER),
        |parent| parent.join(SERVERS_FOLDER),
    ))
}

pub async fn server_dir(id: &str) -> crate::Result<PathBuf> {
    if !is_plain_folder_name(id) {
        return Err(input(format!("Invalid server id: {id}")));
    }
    Ok(servers_dir().await?.join(id))
}

fn input(message: impl Into<String>) -> crate::Error {
    crate::ErrorKind::InputError(message.into()).into()
}

fn is_plain_folder_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\'])
        && Path::new(name).components().count() == 1
}

/// Every server in the servers folder, by name.
#[tracing::instrument]
pub async fn list_servers() -> crate::Result<Vec<HostedServer>> {
    let dir = servers_dir().await?;
    let mut servers = Vec::new();
    let Ok(mut entries) = io::read_dir(&dir).await else {
        return Ok(servers);
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| IOError::with_path(error, &dir))?
    {
        match read_server(&entry.path()).await {
            Ok(Some(server)) => servers.push(server),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                "Skipping server folder {:?}: {error}",
                entry.file_name()
            ),
        }
    }
    servers.sort_by_key(|server| server.name.to_lowercase());
    Ok(servers)
}

pub async fn get_server(id: &str) -> crate::Result<HostedServer> {
    read_server(&server_dir(id).await?)
        .await?
        .ok_or_else(|| input(format!("Unknown server: {id}")))
}

async fn read_server(dir: &Path) -> crate::Result<Option<HostedServer>> {
    let file = dir.join(SERVER_FILE);
    if !file.is_file() {
        return Ok(None);
    }
    let mut server: HostedServer =
        serde_json::from_slice(&io::read(&file).await?)?;
    // The folder name is the id, even if the folder was renamed by hand.
    if let Some(name) = dir.file_name() {
        server.id = name.to_string_lossy().into_owned();
    }
    Ok(Some(server))
}

async fn write_server(server: &HostedServer) -> crate::Result<()> {
    let dir = server_dir(&server.id).await?;
    let temporary = dir.join(format!("{SERVER_FILE}.tmp"));
    io::write(&temporary, serde_json::to_vec_pretty(server)?).await?;
    io::rename_or_move(&temporary, &dir.join(SERVER_FILE)).await?;
    Ok(())
}

/// Builds a server from an instance: the picked files (see the server pack
/// export), the mod loader's server (installed with the matching Java), the
/// world and a `server.properties`.
#[tracing::instrument(skip(request), fields(instance = %request.instance_id))]
pub async fn create_server(
    request: CreateServer,
) -> crate::Result<HostedServer> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(input("Give the server a name"));
    }
    let files = collect_server_files(
        &request.instance_id,
        request.included.clone(),
        request.excluded.clone(),
    )
    .await?;
    let content_set = files.metadata.applied_content_set.clone();

    let root = servers_dir().await?;
    io::create_dir_all(&root).await?;
    let id = free_folder_name(&root, &folder_name(name));
    let dir = root.join(&id);
    io::create_dir_all(&dir).await?;

    let result = async {
        let entries = files.entries;
        let target = dir.clone();
        tokio::task::spawn_blocking(move || write_dir(&target, entries))
            .await??;

        let launch = setup::install_server(
            &dir,
            &content_set.game_version,
            content_set.loader,
            content_set.loader_version.as_deref(),
            &files.launcher_file,
        )
        .await?;

        let port = free_port(&list_servers().await?);
        let mut properties = vec![
            ("server-port".to_string(), port.to_string()),
            ("motd".to_string(), name.to_string()),
            ("level-name".to_string(), "world".to_string()),
        ];
        match &request.world {
            WorldSource::New { seed } => {
                if let Some(seed) = seed.as_deref().filter(|s| !s.is_empty()) {
                    properties
                        .push(("level-seed".to_string(), seed.to_string()));
                }
            }
            WorldSource::Copy { instance_id, world } => {
                let source = crate::api::instance::get_full_path(instance_id)
                    .await?
                    .join("saves")
                    .join(world);
                crate::api::world_transfer::copy_world(
                    &source,
                    &dir.join("world"),
                )
                .await?;
            }
        }
        properties::write_properties(&dir, &properties).await?;

        let server = HostedServer {
            id: id.clone(),
            name: name.to_string(),
            instance_id: Some(request.instance_id.clone()),
            game_version: content_set.game_version.clone(),
            loader: content_set.loader,
            loader_version: content_set.loader_version.clone(),
            memory_mb: request.memory_mb.unwrap_or(DEFAULT_MEMORY_MB),
            port,
            eula_accepted: request.eula_accepted,
            launch,
            selection: Some(ServerPackSelection {
                included: request.included.clone(),
                excluded: request.excluded.clone(),
            }),
            public_access: PublicAccess::Off,
            created: Utc::now(),
        };
        write_server(&server).await?;
        Ok::<_, crate::Error>(server)
    }
    .await;

    if result.is_err() {
        let _ = io::remove_dir_all(&dir).await;
    }
    result
}

/// Changes a server's settings. The port is also written to
/// `server.properties`.
pub async fn edit_server(
    id: &str,
    edit: EditServer,
) -> crate::Result<HostedServer> {
    let mut server = get_server(id).await?;
    if let Some(name) = edit.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(input("Give the server a name"));
        }
        server.name = name.to_string();
    }
    if let Some(memory_mb) = edit.memory_mb {
        server.memory_mb = memory_mb.clamp(512, 65536);
    }
    if let Some(port) = edit.port {
        if port == 0 {
            return Err(input("Pick a port between 1 and 65535"));
        }
        server.port = port;
        properties::set_property(
            &server_dir(id).await?,
            "server-port",
            &port.to_string(),
        )
        .await?;
    }
    if let Some(eula_accepted) = edit.eula_accepted {
        server.eula_accepted = eula_accepted;
    }
    if let Some(public_access) = edit.public_access {
        server.public_access = public_access;
    }
    write_server(&server).await?;
    Ok(server)
}

/// Replaces the server's mods with the instance's current ones (the same
/// picks as when the server was built) and adds config files it doesn't have
/// yet. Configs the server already has are kept, since they may have been
/// changed for the server.
#[tracing::instrument]
pub async fn sync_server_mods(id: &str) -> crate::Result<usize> {
    let server = get_server(id).await?;
    if process::is_running(id) {
        return Err(input("Stop the server before updating its mods"));
    }
    let (Some(instance_id), Some(selection)) =
        (server.instance_id.as_deref(), server.selection.clone())
    else {
        return Err(input("This server wasn't built from an instance"));
    };
    let files = collect_server_files(
        instance_id,
        selection.included,
        selection.excluded,
    )
    .await?;
    let dir = server_dir(id).await?;

    let mods_dir = dir.join("mods");
    if let Ok(mut entries) = io::read_dir(&mods_dir).await {
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| IOError::with_path(error, &mods_dir))?
        {
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "jar") {
                io::remove_file(&path).await?;
            }
        }
    }
    let entries = files
        .entries
        .into_iter()
        .filter(|(name, _)| {
            name.starts_with("mods/") || !dir.join(name).exists()
        })
        .filter(|(name, _)| name.contains('/'))
        .collect::<Vec<_>>();
    let mods = entries
        .iter()
        .filter(|(name, _)| name.starts_with("mods/"))
        .count();
    let target = dir.clone();
    tokio::task::spawn_blocking(move || write_dir(&target, entries)).await??;
    Ok(mods)
}

/// Deletes a stopped server and its folder, world included.
pub async fn delete_server(id: &str) -> crate::Result<()> {
    if process::is_running(id) {
        return Err(input("Stop the server before deleting it"));
    }
    let dir = server_dir(id).await?;
    if dir.join(SERVER_FILE).is_file() {
        io::remove_dir_all(&dir).await?;
    }
    process::forget(id);
    Ok(())
}

/// A folder name for a server name: letters, digits, spaces, dashes and
/// underscores.
fn folder_name(name: &str) -> String {
    let cleaned = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "server".to_string()
    } else {
        cleaned
    }
}

fn free_folder_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    (2..)
        .map(|count| format!("{name} ({count})"))
        .find(|candidate| !dir.join(candidate).exists())
        .expect("an unused name exists")
}

/// 25565, or the next port no other server uses.
fn free_port(servers: &[HostedServer]) -> u16 {
    (DEFAULT_PORT..)
        .find(|port| servers.iter().all(|server| server.port != *port))
        .unwrap_or(DEFAULT_PORT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_are_safe() {
        assert_eq!(folder_name("My Server!"), "My Server_");
        assert_eq!(folder_name("../../etc"), "______etc");
        assert_eq!(folder_name("  "), "server");
        assert!(!is_plain_folder_name("../x"));
        assert!(is_plain_folder_name("My Server"));
    }

    #[test]
    fn ports_skip_used_ones() {
        let server = |port| HostedServer {
            id: String::new(),
            name: String::new(),
            instance_id: None,
            game_version: "1.21.1".to_string(),
            loader: ModLoader::Vanilla,
            loader_version: None,
            memory_mb: DEFAULT_MEMORY_MB,
            port,
            eula_accepted: false,
            launch: LaunchTarget::Jar {
                path: "server.jar".to_string(),
            },
            selection: None,
            public_access: PublicAccess::Off,
            created: Utc::now(),
        };
        assert_eq!(free_port(&[]), 25565);
        assert_eq!(free_port(&[server(25565), server(25566)]), 25567);
    }
}
