//! Installs a mod loader's server into a server folder and finds the Java
//! the server needs.

use super::LaunchTarget;
use crate::state::{JavaVersion, ModLoader, State};
use crate::util::io::{self, IOError};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// The Java a Minecraft version's server needs: an installed one the app
/// knows, or one it downloads (like for playing).
pub(super) async fn server_java(game_version: &str) -> crate::Result<PathBuf> {
    let state = State::get().await?;
    let (minecraft, index) =
        crate::launcher::resolve_minecraft_manifest(game_version, &state)
            .await?;
    let version_info = crate::launcher::download::download_version_info(
        &state,
        &minecraft.versions[index],
        None,
        None,
        None,
        None,
    )
    .await?;
    let major = version_info
        .java_version
        .as_ref()
        .map_or(8, |java| java.major_version);
    if let Some(java) = JavaVersion::get(major, &state.pool).await?
        && Path::new(&java.path).is_file()
    {
        return Ok(PathBuf::from(java.path));
    }
    let path =
        crate::api::jre::auto_install_java_with_loading(major, false).await?;
    let java = crate::api::jre::check_jre(path.clone()).await?;
    java.upsert(&state.pool).await?;
    Ok(path)
}

pub(super) fn java_command(java: &Path) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(java);
    #[cfg(windows)]
    {
        // No console window next to the app.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Runs the loader's installer where one is needed (Quilt, Forge, NeoForge)
/// and returns how to start the server.
pub(super) async fn install_server(
    dir: &Path,
    game_version: &str,
    loader: ModLoader,
    loader_version: Option<&str>,
    launcher_file: &str,
) -> crate::Result<LaunchTarget> {
    match loader {
        ModLoader::Vanilla | ModLoader::Fabric => Ok(LaunchTarget::Jar {
            path: launcher_file.to_string(),
        }),
        ModLoader::Quilt => {
            let version = loader_version.unwrap_or_default();
            run_installer(
                dir,
                game_version,
                &[
                    "-jar",
                    launcher_file,
                    "install",
                    "server",
                    game_version,
                    version,
                    "--download-server",
                    "--install-dir=.",
                ],
            )
            .await?;
            Ok(LaunchTarget::Jar {
                path: "quilt-server-launch.jar".to_string(),
            })
        }
        ModLoader::Forge | ModLoader::NeoForge => {
            run_installer(
                dir,
                game_version,
                &["-jar", launcher_file, "--installServer"],
            )
            .await?;
            forge_launch_target(dir, launcher_file).await
        }
    }
}

async fn run_installer(
    dir: &Path,
    game_version: &str,
    args: &[&str],
) -> crate::Result<()> {
    let java = server_java(game_version).await?;
    let output = java_command(&java)
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| IOError::with_path(error, &java))?;
    let mut log = output.stdout;
    log.extend_from_slice(&output.stderr);
    io::write(dir.join("installer.log"), &log).await?;
    if !output.status.success() {
        let tail = String::from_utf8_lossy(&log)
            .lines()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(crate::ErrorKind::OtherError(format!(
            "The mod loader's server installer failed ({}):\n{tail}",
            output.status
        ))
        .into());
    }
    Ok(())
}

/// Modern Forge and NeoForge installers write `run.sh`, which names the
/// arguments file to start with; older Forge installs a runnable jar.
async fn forge_launch_target(
    dir: &Path,
    installer: &str,
) -> crate::Result<LaunchTarget> {
    if let Ok(script) = tokio::fs::read_to_string(dir.join("run.sh")).await
        && let Some(args) = args_file_in_script(&script)
    {
        return Ok(LaunchTarget::ArgsFile { path: args });
    }
    let mut entries = io::read_dir(dir).await?;
    let mut jars = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| IOError::with_path(error, dir))?
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".jar")
            && name != installer
            && !name.contains("installer")
            && (name.starts_with("forge") || name.starts_with("neoforge"))
        {
            jars.push(name);
        }
    }
    jars.sort();
    jars.pop()
        .map(|path| LaunchTarget::Jar { path })
        .ok_or_else(|| {
            crate::ErrorKind::OtherError(
                "The mod loader installed no server to start".to_string(),
            )
            .into()
        })
}

fn args_file_in_script(script: &str) -> Option<String> {
    script
        .split_whitespace()
        .filter_map(|word| word.trim_matches('"').strip_prefix('@'))
        .find(|path| path.ends_with("unix_args.txt"))
        .map(str::to_string)
}

/// The arguments file for this OS (Windows uses `win_args.txt`).
pub(super) fn platform_args_file(path: &str) -> String {
    if cfg!(windows) {
        path.replace("unix_args.txt", "win_args.txt")
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_forge_args_file() {
        let script = "#!/usr/bin/env sh\n# comment\njava @user_jvm_args.txt @libraries/net/minecraftforge/forge/1.20.1-47.2.0/unix_args.txt \"$@\"\n";
        assert_eq!(
            args_file_in_script(script).as_deref(),
            Some(
                "libraries/net/minecraftforge/forge/1.20.1-47.2.0/unix_args.txt"
            )
        );
        assert_eq!(args_file_in_script("java -jar server.jar"), None);
    }
}
