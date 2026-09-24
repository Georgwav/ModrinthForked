//! Copies or moves a singleplayer world from one instance to another.

use crate::util::io::{self, IOError};
use std::path::{Path, PathBuf};

use super::instance::get_full_path;

#[derive(
    serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum WorldTransferMode {
    Copy,
    Move,
}

/// Copies (or moves) `world` from the `saves` folder of `from_instance` into
/// `to_instance`. The world must not be open in Minecraft. If the target
/// already has a world folder with that name, the copy gets a free name like
/// `World (2)`. Returns the world's folder name in the target instance.
#[tracing::instrument]
pub async fn transfer_world(
    from_instance: &str,
    world: &str,
    to_instance: &str,
    mode: WorldTransferMode,
) -> crate::Result<String> {
    if from_instance == to_instance {
        return Err(crate::ErrorKind::InputError(
            "The world is already in this instance".to_string(),
        )
        .into());
    }
    if !is_plain_folder_name(world) {
        return Err(crate::ErrorKind::InputError(format!(
            "Invalid world folder name: {world}"
        ))
        .into());
    }

    let source = get_full_path(from_instance)
        .await?
        .join("saves")
        .join(world);
    let target_saves = get_full_path(to_instance).await?.join("saves");
    if !source.join("level.dat").is_file() {
        return Err(crate::ErrorKind::InputError(format!(
            "{world} is not a world folder"
        ))
        .into());
    }

    // Holding Minecraft's session lock keeps the game from opening the world
    // while it is copied, and fails if the game has it open right now.
    let lock = super::worlds::get_world_session_lock(&source).await?;

    io::create_dir_all(&target_saves).await?;
    let target_name = free_folder_name(&target_saves, world);
    let target = target_saves.join(&target_name);

    if let Err(error) = copy_world_dir(&source, &target).await {
        // Never leave half a world behind.
        let _ = io::remove_dir_all(&target).await;
        return Err(error);
    }
    drop(lock);

    if mode == WorldTransferMode::Move {
        super::worlds::delete_world(&source_instance_dir(&source), world)
            .await?;
    }

    Ok(target_name)
}

fn source_instance_dir(world_dir: &Path) -> PathBuf {
    world_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn is_plain_folder_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\'])
        && Path::new(name).components().count() == 1
}

/// `name`, or `name (2)`, `name (3)`, ... whichever does not exist in `dir`.
fn free_folder_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    (2..)
        .map(|count| format!("{name} ({count})"))
        .find(|candidate| !dir.join(candidate).exists())
        .expect("an unused name exists")
}

/// Copies a world folder, leaving out Minecraft's `session.lock`.
async fn copy_world_dir(source: &Path, target: &Path) -> crate::Result<()> {
    let mut pending = vec![(source.to_path_buf(), target.to_path_buf())];
    while let Some((from, to)) = pending.pop() {
        io::create_dir_all(&to).await?;
        let mut entries = io::read_dir(&from).await?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| IOError::with_path(error, &from))?
        {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .await
                .map_err(|error| IOError::with_path(error, &path))?;
            let destination = to.join(entry.file_name());
            if file_type.is_dir() {
                pending.push((path, destination));
            } else if file_type.is_file() && entry.file_name() != "session.lock"
            {
                io::copy(&path, &destination).await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_a_free_folder_name() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(free_folder_name(dir.path(), "World"), "World");
        std::fs::create_dir(dir.path().join("World")).unwrap();
        assert_eq!(free_folder_name(dir.path(), "World"), "World (2)");
        std::fs::create_dir(dir.path().join("World (2)")).unwrap();
        assert_eq!(free_folder_name(dir.path(), "World"), "World (3)");
    }

    #[test]
    fn rejects_paths_as_world_names() {
        assert!(is_plain_folder_name("New World"));
        assert!(!is_plain_folder_name("../other"));
        assert!(!is_plain_folder_name("a/b"));
        assert!(!is_plain_folder_name(".."));
        assert!(!is_plain_folder_name(""));
    }

    #[tokio::test]
    async fn copies_world_without_session_lock() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("from");
        std::fs::create_dir_all(source.join("region")).unwrap();
        std::fs::write(source.join("level.dat"), b"level").unwrap();
        std::fs::write(source.join("region/r.0.0.mca"), b"chunks").unwrap();
        std::fs::write(source.join("session.lock"), b"lock").unwrap();

        let target = dir.path().join("to");
        copy_world_dir(&source, &target).await.unwrap();

        assert_eq!(std::fs::read(target.join("level.dat")).unwrap(), b"level");
        assert_eq!(
            std::fs::read(target.join("region/r.0.0.mca")).unwrap(),
            b"chunks"
        );
        assert!(!target.join("session.lock").exists());
    }
}
