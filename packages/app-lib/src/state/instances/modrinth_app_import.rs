//! Instance details from an official Modrinth App install.
//!
//! The Modrinth App keeps each instance's Minecraft version, loader and
//! modpack link only in its own `app.db`, not in the instance folder. When
//! Threadrinth finds such a folder, it looks the folder up there. The
//! database is copied first and only the copy is opened, so the Modrinth App
//! is never affected.

use super::instance_cfg::{InstanceCfg, parse_loader};
use crate::state::InstanceLink;
use chrono::{DateTime, TimeZone, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const MODRINTH_APP_IDENTIFIER: &str = "ModrinthApp";

/// `app.db` locations of an official Modrinth App install.
fn candidate_databases() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(data_dir) = dirs::data_dir() {
        candidates.push(data_dir.join(MODRINTH_APP_IDENTIFIER).join("app.db"));
    }
    #[cfg(target_os = "linux")]
    if let Some(home) = dirs::home_dir() {
        candidates.push(
            home.join(".var/app/com.modrinth.ModrinthApp/data")
                .join(MODRINTH_APP_IDENTIFIER)
                .join("app.db"),
        );
    }
    candidates
}

/// An instance as the official Modrinth App knows it.
#[derive(Clone, Debug)]
pub(crate) struct ModrinthAppInstance {
    pub cfg: InstanceCfg,
    /// The instance's icon in the Modrinth App's cache, if it has one.
    pub icon: Option<PathBuf>,
}

/// Instances known to the official Modrinth App, by folder name. Returns an
/// empty map when there is no Modrinth App install or it can't be read.
pub(crate) async fn load_modrinth_app_instances(
    own_database: &Path,
) -> HashMap<String, ModrinthAppInstance> {
    let own_database = dunce::canonicalize(own_database)
        .unwrap_or_else(|_| own_database.to_path_buf());
    let mut instances = HashMap::new();

    for database in candidate_databases() {
        if !database.is_file()
            || dunce::canonicalize(&database).ok().as_ref()
                == Some(&own_database)
        {
            continue;
        }
        match read_database_copy(&database).await {
            Ok(found) => {
                tracing::info!(
                    "Found {} instances in the Modrinth App at {}",
                    found.len(),
                    database.display()
                );
                let app_dir = database.parent().unwrap_or(Path::new(""));
                for (path, mut instance) in found {
                    // Icons are stored as absolute paths; resolve relative
                    // ones against the Modrinth App's folder just in case.
                    instance.icon = instance
                        .icon
                        .map(|icon| app_dir.join(icon))
                        .filter(|icon| icon.is_file());
                    instances.entry(path).or_insert(instance);
                }
            }
            Err(error) => tracing::warn!(
                "Could not read the Modrinth App database at {}: {error}",
                database.display()
            ),
        }
    }

    instances
}

async fn read_database_copy(
    database: &Path,
) -> crate::Result<HashMap<String, ModrinthAppInstance>> {
    let copy_dir =
        tempfile::tempdir().map_err(crate::util::io::IOError::from)?;
    let copy = copy_dir.path().join("app.db");
    for suffix in ["", "-wal"] {
        let from = PathBuf::from(format!("{}{suffix}", database.display()));
        if from.is_file() {
            let to = PathBuf::from(format!("{}{suffix}", copy.display()));
            tokio::fs::copy(&from, &to).await.map_err(|error| {
                crate::util::io::IOError::with_path(error, &from)
            })?;
        }
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&copy))
        .await?;
    let result = match read_instances(&pool).await {
        Ok(instances) => Ok(instances),
        Err(_) => read_legacy_profiles(&pool).await,
    };
    pool.close().await;
    result
}

/// Modrinth App 0.21 and newer: `instances` + `instance_content_sets`.
pub(crate) async fn read_instances(
    pool: &SqlitePool,
) -> crate::Result<HashMap<String, ModrinthAppInstance>> {
    let rows = sqlx::query(
        "SELECT i.path, i.name, i.icon_path, i.created, i.last_played,
			i.submitted_time_played, i.recent_time_played,
			cs.game_version, cs.loader, cs.loader_version,
			l.link_kind, l.modrinth_project_id, l.modrinth_version_id
		FROM instances i
		JOIN instance_content_sets cs ON cs.id = i.applied_content_set_id
		LEFT JOIN instance_links l ON l.instance_id = i.id",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let link = match (
                row.try_get::<Option<String>, _>("link_kind").ok().flatten(),
                row.try_get::<Option<String>, _>("modrinth_project_id")
                    .ok()
                    .flatten(),
                row.try_get::<Option<String>, _>("modrinth_version_id")
                    .ok()
                    .flatten(),
            ) {
                (Some(kind), Some(project_id), Some(version_id))
                    if kind == "modrinth_modpack" =>
                {
                    Some(InstanceLink::ModrinthModpack {
                        project_id,
                        version_id,
                    })
                }
                _ => None,
            };
            to_instance(&row, "loader", "loader_version", link)
        })
        .collect())
}

/// Modrinth App before 0.21: a single `profiles` table.
pub(crate) async fn read_legacy_profiles(
    pool: &SqlitePool,
) -> crate::Result<HashMap<String, ModrinthAppInstance>> {
    let rows = sqlx::query(
        "SELECT path, name, icon_path, created, last_played, submitted_time_played,
			recent_time_played, game_version, mod_loader, mod_loader_version,
			linked_project_id, linked_version_id
		FROM profiles",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let link = match (
                row.try_get::<Option<String>, _>("linked_project_id")
                    .ok()
                    .flatten(),
                row.try_get::<Option<String>, _>("linked_version_id")
                    .ok()
                    .flatten(),
            ) {
                (Some(project_id), Some(version_id))
                    if !version_id.is_empty() =>
                {
                    Some(InstanceLink::ModrinthModpack {
                        project_id,
                        version_id,
                    })
                }
                _ => None,
            };
            to_instance(&row, "mod_loader", "mod_loader_version", link)
        })
        .collect())
}

fn to_instance(
    row: &sqlx::sqlite::SqliteRow,
    loader_column: &str,
    loader_version_column: &str,
    link: Option<InstanceLink>,
) -> Option<(String, ModrinthAppInstance)> {
    let path: String = row.try_get("path").ok()?;
    let game_version: String = row.try_get("game_version").ok()?;
    if path.is_empty() || game_version.is_empty() {
        return None;
    }
    let timestamp = |column: &str| -> Option<DateTime<Utc>> {
        let seconds: Option<i64> = row.try_get(column).ok().flatten();
        seconds.and_then(|seconds| Utc.timestamp_opt(seconds, 0).single())
    };
    let playtime = |column: &str| -> u64 {
        row.try_get::<i64, _>(column)
            .ok()
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0)
    };

    let icon = row
        .try_get::<Option<String>, _>("icon_path")
        .ok()
        .flatten()
        .filter(|icon| !icon.is_empty())
        .map(PathBuf::from);

    let cfg = InstanceCfg {
        id: None,
        name: row.try_get("name").unwrap_or_else(|_| path.clone()),
        game_version,
        loader: row
            .try_get::<String, _>(loader_column)
            .ok()
            .as_deref()
            .and_then(parse_loader)
            .unwrap_or(crate::state::ModLoader::Vanilla),
        loader_version: row
            .try_get::<Option<String>, _>(loader_version_column)
            .ok()
            .flatten()
            .filter(|value| !value.is_empty()),
        created: timestamp("created"),
        modified: None,
        last_played: timestamp("last_played"),
        submitted_time_played: playtime("submitted_time_played"),
        recent_time_played: playtime("recent_time_played"),
        link,
        icon: None,
    };
    Some((path, ModrinthAppInstance { cfg, icon }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ModLoader;

    async fn memory_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn reads_current_modrinth_app_schema() {
        let pool = memory_pool().await;
        sqlx::migrate!().run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO instances (id, path, applied_content_set_id, install_stage, launcher_feature_version, name, icon_path, created, modified, last_played, submitted_time_played)
			VALUES ('local:a', 'Horror OneBlock', 'cs:a', 'installed', 'none', 'Horror OneBlock', '/icons/horror.png', 1700000000, 1700000000, 1700000500, 7200)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO instance_content_sets (id, instance_id, name, source_kind, status, game_version, loader, loader_version, created, modified)
			VALUES ('cs:a', 'local:a', 'Default', 'modrinth_modpack', 'available', '26.1.2', 'fabric', '0.17.2', 1700000000, 1700000000)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO instance_links (instance_id, link_kind, modrinth_project_id, modrinth_version_id)
			VALUES ('local:a', 'modrinth_modpack', 'proj', 'ver')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let found = read_instances(&pool).await.unwrap();
        let found_instance = &found["Horror OneBlock"];
        assert_eq!(
            found_instance.icon.as_deref(),
            Some(Path::new("/icons/horror.png"))
        );
        let cfg = &found_instance.cfg;
        assert_eq!(cfg.game_version, "26.1.2");
        assert_eq!(cfg.loader, ModLoader::Fabric);
        assert_eq!(cfg.loader_version.as_deref(), Some("0.17.2"));
        assert_eq!(cfg.submitted_time_played, 7200);
        assert!(matches!(
            &cfg.link,
            Some(InstanceLink::ModrinthModpack { project_id, version_id })
                if project_id == "proj" && version_id == "ver"
        ));
    }

    #[tokio::test]
    async fn reads_legacy_profiles_table() {
        let pool = memory_pool().await;
        sqlx::query(
            "CREATE TABLE profiles (path TEXT, name TEXT, icon_path TEXT, created INTEGER, last_played INTEGER, submitted_time_played INTEGER, recent_time_played INTEGER, game_version TEXT, mod_loader TEXT, mod_loader_version TEXT, linked_project_id TEXT, linked_version_id TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO profiles VALUES ('Sodium Plus', 'Sodium Plus', NULL, 1700000000, NULL, 60, 5, '1.21.11', 'neoforge', '21.11.3', NULL, NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let found = read_legacy_profiles(&pool).await.unwrap();
        let cfg = &found["Sodium Plus"].cfg;
        assert_eq!(cfg.game_version, "1.21.11");
        assert_eq!(cfg.loader, ModLoader::NeoForge);
        assert_eq!(cfg.loader_version.as_deref(), Some("21.11.3"));
        assert_eq!(cfg.recent_time_played, 5);
        assert!(cfg.link.is_none());
    }
}
