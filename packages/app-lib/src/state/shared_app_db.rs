//! Keeps `app.db` in the app folder when one is chosen in settings.
//!
//! Normally the database lives in the OS settings folder, apart from the app
//! folder (instances, game files, caches). With a custom app folder it moves
//! into that folder instead, so every install using the folder (another
//! computer, or the other OS of a dual boot) shares settings, accounts,
//! skins and the mod index. A small pointer file in the settings folder says
//! where the database is, since the app folder setting itself is stored in
//! the database.

use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

const POINTER_FILE: &str = "app-folder.txt";
const DB_FILE: &str = "app.db";

/// The app folder holding the database, if one is set and reachable (a drive
/// that isn't plugged in counts as not set).
pub(crate) fn shared_folder(settings_dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(settings_dir.join(POINTER_FILE)).ok()?;
    let folder = PathBuf::from(text.trim());
    (folder.is_absolute() && folder.is_dir()).then_some(folder)
}

/// Where to open `app.db`.
pub(crate) async fn app_db_path(settings_dir: &Path) -> PathBuf {
    let local = settings_dir.join(DB_FILE);
    let Some(folder) = shared_folder(settings_dir) else {
        return local;
    };
    let shared = folder.join(DB_FILE);
    if !shared.is_file()
        && local.is_file()
        && let Err(error) = copy_database_files(&local, &shared).await
    {
        tracing::warn!(
            "Could not copy the database into {}; using this install's own: {error}",
            folder.display()
        );
        return local;
    }
    shared
}

/// For installs that chose a custom app folder before the database moved
/// with it: moves the database there now (or adopts the one already there,
/// written by another install). Returns the database to switch to.
pub(crate) async fn adopt_custom_folder(
    settings_dir: &Path,
    custom_dir: Option<&str>,
    pool: &SqlitePool,
) -> crate::Result<Option<PathBuf>> {
    if settings_dir.join(POINTER_FILE).exists() {
        return Ok(None);
    }
    let Some(folder) = custom_dir.map(PathBuf::from) else {
        return Ok(None);
    };
    if !folder.is_dir() || same_folder(&folder, settings_dir).await {
        return Ok(None);
    }
    let shared = folder.join(DB_FILE);
    if !shared.is_file() {
        snapshot_into(pool, &shared).await?;
    }
    write_pointer(settings_dir, &folder).await?;
    tracing::info!("Using the app database in {}", folder.display());
    Ok(Some(shared))
}

/// Called after the app folder changed: the database follows it (taking the
/// one already in the new folder, if another install left one), or goes back
/// to the settings folder when the default folder is chosen again. Takes
/// effect on the next start.
pub(crate) async fn app_folder_changed(
    settings_dir: &Path,
    destination: &Path,
    pool: &SqlitePool,
) -> crate::Result<()> {
    let current = shared_folder(settings_dir);
    if same_folder(destination, settings_dir).await {
        if current.is_some() {
            let local = settings_dir.join(DB_FILE);
            for suffix in ["", "-wal", "-shm"] {
                let _ =
                    tokio::fs::remove_file(with_suffix(&local, suffix)).await;
            }
            snapshot_into(pool, &local).await?;
        }
        let _ = tokio::fs::remove_file(settings_dir.join(POINTER_FILE)).await;
        return Ok(());
    }
    if let Some(current) = &current
        && same_folder(current, destination).await
    {
        return Ok(());
    }
    let shared = destination.join(DB_FILE);
    if !shared.is_file() {
        snapshot_into(pool, &shared).await?;
    }
    write_pointer(settings_dir, destination).await
}

async fn write_pointer(
    settings_dir: &Path,
    folder: &Path,
) -> crate::Result<()> {
    let temporary = settings_dir.join(format!("{POINTER_FILE}.tmp"));
    tokio::fs::write(&temporary, folder.to_string_lossy().as_bytes()).await?;
    tokio::fs::rename(&temporary, settings_dir.join(POINTER_FILE)).await?;
    Ok(())
}

/// Writes a consistent copy of the open database to `target`.
async fn snapshot_into(pool: &SqlitePool, target: &Path) -> crate::Result<()> {
    let temporary = with_suffix(target, ".tmp");
    let _ = tokio::fs::remove_file(&temporary).await;
    sqlx::query("VACUUM INTO ?")
        .bind(temporary.to_string_lossy().into_owned())
        .execute(pool)
        .await?;
    tokio::fs::rename(&temporary, target).await?;
    Ok(())
}

/// Copies a closed database (with its write-ahead log, if any).
async fn copy_database_files(from: &Path, to: &Path) -> crate::Result<()> {
    let wal = with_suffix(from, "-wal");
    if wal.is_file() {
        tokio::fs::copy(&wal, with_suffix(to, "-wal")).await?;
    }
    let temporary = with_suffix(to, ".tmp");
    tokio::fs::copy(from, &temporary).await?;
    tokio::fs::rename(&temporary, to).await?;
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

async fn same_folder(a: &Path, b: &Path) -> bool {
    match (
        tokio::fs::canonicalize(a).await,
        tokio::fs::canonicalize(b).await,
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    async fn pool_at(path: &Path) -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true),
            )
            .await
            .unwrap()
    }

    async fn marker(pool: &SqlitePool) -> String {
        sqlx::query_scalar("SELECT value FROM marker")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn database_follows_the_app_folder_and_back() {
        let settings_dir = tempfile::tempdir().unwrap();
        let app_folder = tempfile::tempdir().unwrap();
        let local_pool = pool_at(&settings_dir.path().join(DB_FILE)).await;
        sqlx::query("CREATE TABLE marker (value TEXT)")
            .execute(&local_pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO marker VALUES ('local')")
            .execute(&local_pool)
            .await
            .unwrap();

        // No custom folder: the database stays in the settings folder.
        assert_eq!(
            app_db_path(settings_dir.path()).await,
            settings_dir.path().join(DB_FILE)
        );

        // Choosing a custom folder copies the database there.
        app_folder_changed(settings_dir.path(), app_folder.path(), &local_pool)
            .await
            .unwrap();
        let shared_path = app_db_path(settings_dir.path()).await;
        assert_eq!(shared_path, app_folder.path().join(DB_FILE));
        let shared_pool = pool_at(&shared_path).await;
        assert_eq!(marker(&shared_pool).await, "local");

        assert_eq!(
            shared_folder(settings_dir.path()),
            Some(app_folder.path().to_path_buf())
        );

        // Going back to the default folder brings the database home.
        sqlx::query("UPDATE marker SET value = 'shared'")
            .execute(&shared_pool)
            .await
            .unwrap();
        local_pool.close().await;
        app_folder_changed(
            settings_dir.path(),
            settings_dir.path(),
            &shared_pool,
        )
        .await
        .unwrap();
        assert!(shared_folder(settings_dir.path()).is_none());
        let home = pool_at(&app_db_path(settings_dir.path()).await).await;
        assert_eq!(marker(&home).await, "shared");
    }

    #[tokio::test]
    async fn a_database_already_in_the_folder_is_adopted() {
        let settings_dir = tempfile::tempdir().unwrap();
        let app_folder = tempfile::tempdir().unwrap();
        let other_install = pool_at(&app_folder.path().join(DB_FILE)).await;
        sqlx::query("CREATE TABLE marker (value TEXT)")
            .execute(&other_install)
            .await
            .unwrap();
        sqlx::query("INSERT INTO marker VALUES ('other install')")
            .execute(&other_install)
            .await
            .unwrap();
        other_install.close().await;

        let this_install = pool_at(&settings_dir.path().join(DB_FILE)).await;
        let custom_dir = app_folder.path().to_string_lossy().into_owned();
        let switched_to = adopt_custom_folder(
            settings_dir.path(),
            Some(&custom_dir),
            &this_install,
        )
        .await
        .unwrap();
        assert_eq!(switched_to, Some(app_folder.path().join(DB_FILE)));
        let shared = pool_at(&app_db_path(settings_dir.path()).await).await;
        assert_eq!(marker(&shared).await, "other install");
    }
}
