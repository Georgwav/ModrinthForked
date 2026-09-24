use crate::state::DirectoryInfo;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions,
};
use sqlx::{Pool, Sqlite};
use std::path::Path;
use std::time::Duration;

pub(crate) async fn connect(
    app_identifier: &str,
) -> crate::Result<Pool<Sqlite>> {
    let settings_dir = DirectoryInfo::initial_settings_dir_path(app_identifier)
        .ok_or(crate::ErrorKind::FSError(
            "Could not find valid config dir".to_string(),
        ))?;

    crate::util::io::create_dir_all(&settings_dir).await?;

    let db_path = settings_dir.join("app.db");

    connect_app_db(&db_path).await
}

async fn connect_app_db(db_path: &Path) -> crate::Result<Pool<Sqlite>> {
    super::db_backup::maybe_backup_existing_app_db(db_path).await?;
    match open_migrated_app_db(db_path).await {
        Err(OpenError::IncompatibleMigrations(error)) => {
            set_aside_incompatible_app_db(db_path, &error).await?;
            open_migrated_app_db(db_path)
                .await
                .map_err(OpenError::into_error)
        }
        result => result.map_err(OpenError::into_error),
    }
}

enum OpenError {
    /// The database was migrated with a migration of the same version but
    /// different contents, so its schema cannot be trusted by this build.
    IncompatibleMigrations(sqlx::migrate::MigrateError),
    Other(crate::Error),
}

impl OpenError {
    fn into_error(self) -> crate::Error {
        match self {
            Self::IncompatibleMigrations(error) => error.into(),
            Self::Other(error) => error,
        }
    }
}

impl From<crate::Error> for OpenError {
    fn from(error: crate::Error) -> Self {
        Self::Other(error)
    }
}

async fn open_migrated_app_db(
    db_path: &Path,
) -> Result<Pool<Sqlite>, OpenError> {
    let pool = open_app_db_pool(db_path).await?;

    if let Err(err) = stale_data_cleanup(&pool).await {
        tracing::warn!(
            "Failed to clean up stale data from state database before migrations: {err}"
        );
    }

    let mut migrator = sqlx::migrate!();
    // A database last opened by a build that ships more migrations (another
    // OS, a newer version) must still open here instead of being refused.
    migrator.set_ignore_missing(true);
    if let Err(err) = reconcile_migration_checksums(&pool, &migrator).await {
        tracing::warn!("Failed to reconcile migration checksums: {err}");
    }
    if let Err(error) = migrator.run(&pool).await {
        pool.close().await;
        return Err(match error {
            sqlx::migrate::MigrateError::VersionMismatch(_) => {
                OpenError::IncompatibleMigrations(error)
            }
            error => OpenError::Other(error.into()),
        });
    }
    record_current_app_version(&pool).await?;

    if let Err(err) = stale_data_cleanup(&pool).await {
        tracing::warn!(
            "Failed to clean up stale data from state database: {err}"
        );
    }

    Ok(pool)
}

/// SQLx refuses to run when an applied migration's checksum differs from the
/// one compiled into the app. The checksum covers the raw file bytes, so the
/// same migration checked out with CRLF (Windows) or LF (Linux) line endings
/// gets a different checksum, and a database written by one OS's build is
/// refused by the other's. Applied migrations that only differ in line
/// endings are recorded with this build's checksum so they are accepted.
async fn reconcile_migration_checksums(
    pool: &Pool<Sqlite>,
    migrator: &sqlx::migrate::Migrator,
) -> crate::Result<()> {
    use sha2::{Digest, Sha384};

    let has_migrations_table: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await?;
    if !has_migrations_table {
        return Ok(());
    }

    let applied: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations")
            .fetch_all(pool)
            .await?;
    let applied: std::collections::HashMap<i64, Vec<u8>> =
        applied.into_iter().collect();

    for migration in migrator.iter() {
        if migration.migration_type.is_down_migration() {
            continue;
        }
        let Some(applied_checksum) = applied.get(&migration.version) else {
            continue;
        };
        let checksum: &[u8] = &migration.checksum;
        if applied_checksum.as_slice() == checksum {
            continue;
        }

        let lf = migration.sql.replace("\r\n", "\n");
        let crlf = lf.replace('\n', "\r\n");
        let differs_only_in_line_endings = [lf, crlf].iter().any(|sql| {
            Sha384::digest(sql.as_bytes())[..] == applied_checksum[..]
        });
        if !differs_only_in_line_endings {
            continue;
        }

        tracing::info!(
            version = migration.version,
            "Accepting applied migration that differs only in line endings"
        );
        sqlx::query(
            "UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?",
        )
        .bind(checksum)
        .bind(migration.version)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Moves a database this build cannot migrate out of the way, keeping it next
/// to the original so nothing is lost. Instances are restored on the next
/// startup from the instance.cfg in each instance folder.
async fn set_aside_incompatible_app_db(
    db_path: &Path,
    error: &sqlx::migrate::MigrateError,
) -> crate::Result<()> {
    let suffix = format!(
        "incompatible-{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    );
    tracing::error!(
        "The app database cannot be used by this version ({error}); keeping it as app.db.{suffix} and starting with a new one"
    );

    for extension in ["", "-wal", "-shm"] {
        let from = db_path.with_file_name(format!("app.db{extension}"));
        if !from.exists() {
            continue;
        }
        let to = db_path.with_file_name(format!("app.db.{suffix}{extension}"));
        tokio::fs::rename(&from, &to).await.map_err(|error| {
            crate::util::io::IOError::with_path(error, &from)
        })?;
    }

    Ok(())
}

async fn open_app_db_pool(db_path: &Path) -> crate::Result<Pool<Sqlite>> {
    let conn_options = SqliteConnectOptions::new()
        .filename(db_path)
        .busy_timeout(Duration::from_secs(30))
        .journal_mode(SqliteJournalMode::Wal)
        .optimize_on_close(true, None)
        .create_if_missing(true);

    Ok(SqlitePoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .idle_timeout(Duration::from_secs(120))
        .connect_with(conn_options)
        .await?)
}

async fn record_current_app_version(pool: &Pool<Sqlite>) -> crate::Result<()> {
    let previous_version = sqlx::query_scalar!(
        "SELECT value FROM app_metadata WHERE key = 'app_version'"
    )
    .fetch_optional(pool)
    .await?;
    let already_used_sync_update = previous_version
        .as_deref()
        .and_then(|version| {
            let mut parts = version.split('.');
            Some((
                parts.next()?.parse::<u64>().ok()?,
                parts.next()?.parse::<u64>().ok()?,
            ))
        })
        .is_some_and(|version| version >= (0, 20));

    if env!("CARGO_PKG_VERSION").starts_with("0.20.")
        && already_used_sync_update
    {
        let mut settings = super::Settings::get(pool).await?;
        if settings.pending_update_toast_for_version.is_some() {
            settings.pending_update_toast_for_version = None;
            settings.update(pool).await?;
        }
    }

    sqlx::query!(
        "
		INSERT INTO app_metadata (key, value, updated_at)
		VALUES ('app_version', ?, unixepoch())
		ON CONFLICT(key) DO UPDATE SET
			value = excluded.value,
			updated_at = excluded.updated_at
		",
        env!("CARGO_PKG_VERSION"),
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Cleans up data from the database that is no longer referenced, but must be
/// kept around for a little while to allow users to recover from accidental
/// deletions.
async fn stale_data_cleanup(pool: &Pool<Sqlite>) -> crate::Result<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

    let has_skin_tables = sqlx::query!(
		"SELECT COUNT(*) AS \"count!: i64\" FROM sqlite_master WHERE type = 'table' AND name IN ('custom_minecraft_skins', 'minecraft_users')",
	)
	.fetch_one(&mut *tx)
	.await?
	.count == 2;

    if has_skin_tables {
        sqlx::query!(
			"DELETE FROM custom_minecraft_skins WHERE minecraft_user_uuid NOT IN (SELECT uuid FROM minecraft_users)"
		)
		.execute(&mut *tx)
		.await?;
    }

    tx.commit().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha384};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn memory_pool() -> Pool<Sqlite> {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    /// A database migrated by a build whose migration files had the other
    /// line endings (Windows vs Linux) must still open.
    #[tokio::test]
    async fn accepts_migrations_differing_only_in_line_endings() {
        let pool = memory_pool().await;
        let migrator = sqlx::migrate!();
        migrator.run(&pool).await.unwrap();

        let migration = migrator.iter().next().unwrap();
        let other_os_sql = if migration.sql.contains("\r\n") {
            migration.sql.replace("\r\n", "\n")
        } else {
            migration.sql.replace('\n', "\r\n")
        };
        let other_os_checksum =
            Sha384::digest(other_os_sql.as_bytes()).to_vec();
        sqlx::query(
            "UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?",
        )
        .bind(&other_os_checksum)
        .bind(migration.version)
        .execute(&pool)
        .await
        .unwrap();
        assert!(matches!(
            migrator.run(&pool).await,
            Err(sqlx::migrate::MigrateError::VersionMismatch(_))
        ));

        reconcile_migration_checksums(&pool, &migrator)
            .await
            .unwrap();
        migrator.run(&pool).await.unwrap();
    }

    /// Genuinely different migrations are not silently accepted.
    #[tokio::test]
    async fn keeps_rejecting_changed_migrations() {
        let pool = memory_pool().await;
        let migrator = sqlx::migrate!();
        migrator.run(&pool).await.unwrap();

        let migration = migrator.iter().next().unwrap();
        sqlx::query(
            "UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?",
        )
        .bind(vec![0u8; 48])
        .bind(migration.version)
        .execute(&pool)
        .await
        .unwrap();

        reconcile_migration_checksums(&pool, &migrator)
            .await
            .unwrap();
        assert!(matches!(
            migrator.run(&pool).await,
            Err(sqlx::migrate::MigrateError::VersionMismatch(_))
        ));
    }

    /// Migrations applied by a build that ships more of them do not block
    /// opening the database.
    #[tokio::test]
    async fn ignores_migrations_from_other_builds() {
        let pool = memory_pool().await;
        let mut migrator = sqlx::migrate!();
        migrator.run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
            VALUES (99999999999999, 'from another build', TRUE, X'00', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        migrator.set_ignore_missing(true);
        migrator.run(&pool).await.unwrap();
    }
}
