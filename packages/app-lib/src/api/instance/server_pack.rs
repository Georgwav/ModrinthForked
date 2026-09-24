//! Exports an instance as a ready-to-run server: the mods a server needs,
//! their configs, the mod loader's server launcher and start scripts.

use super::export_mrpack::export_content;
use super::{create_mrpack_json, get, get_full_path};
use crate::pack::install_from::EnvType;
use crate::state::content_store::{ReadableContent, content_file_path};
use crate::state::instances::adapters::sqlite::content_rows;
use crate::state::{ModLoader, SideType, State};
use crate::util::fetch::{fetch, fetch_json};
use crate::util::io::{self, IOError};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Folders copied into the server pack, besides `mods`. Everything else
/// (worlds, resource packs, shaders, screenshots, client options) stays out.
const SERVER_FOLDERS: &[&str] = &[
    "config",
    "defaultconfigs",
    "kubejs",
    "scripts",
    "global_packs",
    "datapacks",
];

/// What the export put in, and which mods it left out.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ServerPackReport {
    pub mods_included: usize,
    /// Mods Modrinth lists as client-only, which a server can't load.
    pub client_only_mods: Vec<String>,
    /// Mods not on Modrinth: included, but they may be client-only.
    pub unknown_mods: Vec<String>,
}

enum Entry {
    Content(ReadableContent),
    Bytes(Vec<u8>, bool),
}

/// Writes the server pack for `instance_id` as a zip to `export_path`.
#[tracing::instrument]
pub async fn export_server_pack(
    instance_id: &str,
    export_path: PathBuf,
) -> crate::Result<ServerPackReport> {
    let state = State::get().await?;
    let metadata = get(instance_id).await?.ok_or_else(|| {
        crate::ErrorKind::InputError("Unknown instance".to_string())
    })?;
    let instance_dir = get_full_path(instance_id).await?;
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

    let content_set = &metadata.applied_content_set;
    let game_version = content_set.game_version.clone();
    let loader = content_set.loader;
    let loader_version = content_set.loader_version.clone();

    // Modrinth tells which mods are client-only.
    let pack = create_mrpack_json(&metadata, "1.0.0".to_string(), None).await?;
    let known_mods = pack
        .files
        .iter()
        .map(|file| file.path.as_str().to_string())
        .collect::<HashSet<_>>();
    let client_only = pack
        .files
        .iter()
        .filter(|file| {
            file.env.as_ref().and_then(|env| env.get(&EnvType::Server))
                == Some(&SideType::Unsupported)
        })
        .map(|file| file.path.as_str().to_string())
        .collect::<HashSet<_>>();

    let stored_files =
        content_rows::get_instance_files(instance_id, &state.pool)
            .await?
            .into_iter()
            .map(|file| (content_file_path(&file), file))
            .collect::<HashMap<_, _>>();

    let mut report = ServerPackReport::default();
    let mut entries: Vec<(String, Entry)> = Vec::new();
    for folder in std::iter::once("mods").chain(SERVER_FOLDERS.iter().copied())
    {
        let mut pending = vec![instance_dir.join(folder)];
        while let Some(dir) = pending.pop() {
            let Ok(mut read_dir) = io::read_dir(&dir).await else {
                continue;
            };
            while let Some(entry) = read_dir
                .next_entry()
                .await
                .map_err(|error| IOError::with_path(error, &dir))?
            {
                let path = entry.path();
                let file_type = entry
                    .file_type()
                    .await
                    .map_err(|error| IOError::with_path(error, &path))?;
                if file_type.is_dir() {
                    pending.push(path);
                    continue;
                }
                let relative = relative_path(&instance_dir, &path);
                if folder == "mods" {
                    // Only enabled jars directly in mods/.
                    if !relative.ends_with(".jar")
                        || relative.matches('/').count() != 1
                    {
                        continue;
                    }
                    let file_name =
                        relative.trim_start_matches("mods/").to_string();
                    if client_only.contains(&relative) {
                        report.client_only_mods.push(file_name);
                        continue;
                    }
                    if known_mods.contains(&relative) {
                        report.mods_included += 1;
                    } else {
                        report.unknown_mods.push(file_name);
                    }
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
    }
    report.mods_included += report.unknown_mods.len();
    report.client_only_mods.sort();
    report.unknown_mods.sort();

    let launcher = server_launcher(
        &state,
        &game_version,
        loader,
        loader_version.as_deref(),
    )
    .await?;
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
            readme(&metadata.instance.name, &game_version, &report)
                .into_bytes(),
            false,
        ),
    ));

    tokio::task::spawn_blocking(move || write_zip(&export_path, entries))
        .await??;
    Ok(report)
}

fn relative_path(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
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
    let mut text = format!(
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
    );
    if !report.client_only_mods.is_empty() {
        text.push_str("\nLeft out because they only work on the client:\n");
        for name in &report.client_only_mods {
            let _ = writeln!(text, "  {name}");
        }
    }
    if !report.unknown_mods.is_empty() {
        text.push_str(
            "\nNot on Modrinth, so included without checking (remove any that are client-only if the server won't start):\n",
        );
        for name in &report.unknown_mods {
            let _ = writeln!(text, "  {name}");
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_lists_left_out_mods() {
        let report = ServerPackReport {
            mods_included: 3,
            client_only_mods: vec!["sodium.jar".to_string()],
            unknown_mods: vec!["custom.jar".to_string()],
        };
        let text = readme("Pack", "1.21.1", &report);
        assert!(text.contains("Mods included: 3"));
        assert!(text.contains("  sodium.jar"));
        assert!(text.contains("  custom.jar"));
    }

    #[test]
    fn relative_paths_use_forward_slashes() {
        let base = Path::new("/instances/pack");
        assert_eq!(
            relative_path(base, &base.join("config").join("a.toml")),
            "config/a.toml"
        );
    }
}
