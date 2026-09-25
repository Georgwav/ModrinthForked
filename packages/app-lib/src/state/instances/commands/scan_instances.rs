//! Prism-style instance discovery.
//!
//! The instances directory is the source of truth for which instances exist:
//! every folder in it is matched against `app.db`, and folders the database
//! does not know about are imported from their `instance.cfg` (or from what
//! can be detected in the folder). Nothing is ever deleted here: database rows
//! whose folder is gone are left alone, and folders that cannot be imported
//! safely are skipped and logged.

use super::edit_instance::{AppliedContentSetPatch, EditInstance};
use crate::event::InstancePayloadType;
use crate::event::emit::emit_instance;
use crate::state::instances::instance_cfg::{
    self, CfgRead, InstanceCfg, infer_instance_cfg, is_nested_prism_instance,
    looks_like_instance, read_instance_cfg, write_instance_cfg,
};
use crate::state::instances::modrinth_app_import::{
    ModrinthAppInstance, load_modrinth_app_instances,
};
use crate::state::instances::{
    ContentSet, ContentSetStatus, Instance, InstanceLaunchOverrides,
    InstanceLink,
    adapters::sqlite::{content_rows, instance_rows},
};
use crate::state::{
    DirectoryInfo, InstanceInstallStage, LauncherFeatureVersion,
    ReleaseChannel, State,
};
use chrono::Utc;
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

/// What a scan of the instances directory changed.
#[derive(Clone, Debug, Default, Serialize)]
pub struct InstanceScanReport {
    /// Ids of folders that were added to the database as new instances.
    pub imported: Vec<String>,
    /// Ids of instances whose folder was renamed or moved while the app was
    /// closed.
    pub relocated: Vec<String>,
    /// Ids of instances whose database row was updated from a newer
    /// `instance.cfg`.
    pub updated_from_cfg: Vec<String>,
    /// Folders that look like instances but could not be imported, with the
    /// reason.
    pub skipped: Vec<(String, String)>,
}

impl InstanceScanReport {
    pub fn changed(&self) -> bool {
        !self.imported.is_empty()
            || !self.relocated.is_empty()
            || !self.updated_from_cfg.is_empty()
    }
}

pub(crate) async fn scan_instances_folder(
    state: &State,
) -> crate::Result<InstanceScanReport> {
    let instances_dir = state.directories.instances_dir();
    let mut report = InstanceScanReport::default();

    let mut entries = match tokio::fs::read_dir(&instances_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(report);
        }
        Err(error) => {
            return Err(crate::util::io::IOError::with_path(
                error,
                &instances_dir,
            )
            .into());
        }
    };

    let rows = instance_rows::list_instances(&state.pool).await?;
    let mut rows_by_path: HashMap<String, Instance> = HashMap::new();
    let mut rows_by_id: HashMap<String, Instance> = HashMap::new();
    for row in rows {
        rows_by_path.insert(row.path.clone(), row.clone());
        rows_by_id.insert(row.id.clone(), row);
    }

    let mut folders = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|error| {
        crate::util::io::IOError::with_path(error, &instances_dir)
    })? {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(ToString::to_string)
        else {
            continue;
        };
        if name.starts_with('.')
            || crate::state::content_store::validate_instance_path(&name)
                .is_err()
        {
            continue;
        }
        folders.push(name);
    }
    folders.sort();

    let modrinth_app = if folders
        .iter()
        .any(|folder| rows_by_path.get(folder).is_none_or(|row| !has_icon(row)))
    {
        load_modrinth_app_instances(
            &state.directories.settings_dir.join("app.db"),
        )
        .await
    } else {
        HashMap::new()
    };

    for folder in folders {
        let dir = instances_dir.join(&folder);
        let result = match rows_by_path.get(&folder) {
            Some(row) => {
                sync_known_folder(row, &dir, &state.pool, &mut report).await
            }
            None => {
                import_unknown_folder(
                    &folder,
                    &dir,
                    &instances_dir,
                    &modrinth_app,
                    &mut rows_by_id,
                    state,
                    &mut report,
                )
                .await
            }
        };

        if let Err(error) = result {
            tracing::warn!(
                "Could not scan instance folder {folder:?}: {error}"
            );
            report.skipped.push((folder, error.to_string()));
        }
    }

    sync_folder_icons(&instances_dir, &modrinth_app, state).await;

    if report.changed() || !report.skipped.is_empty() {
        tracing::info!(
            imported = report.imported.len(),
            relocated = report.relocated.len(),
            updated_from_cfg = report.updated_from_cfg.len(),
            skipped = report.skipped.len(),
            "Scanned instances folder",
        );
    }

    emit_scan_events(&report).await;
    queue_minecraft_installs(&report);
    if !report.imported.is_empty() || !report.relocated.is_empty() {
        // Found instances count as having one, so the welcome screen gives
        // way to the library.
        if let Err(error) =
            crate::api::onboarding_checklist::mark_created_instance().await
        {
            tracing::warn!(
                "Could not update the onboarding checklist: {error}"
            );
        }
    }

    Ok(report)
}

/// Whether the instance has an icon file this install can show. A path from
/// another install (for example another OS sharing the app folder) counts as
/// no icon.
fn has_icon(instance: &Instance) -> bool {
    instance
        .icon_path
        .as_deref()
        .is_some_and(|icon| !icon.is_empty() && Path::new(icon).is_file())
}

/// The instance's icon, kept in its own folder so it travels with it.
const FOLDER_ICON: &str = "icon.png";

/// Keeps each instance's icon and its folder's `icon.png` in step. The
/// folder wins: an `icon.png` that differs from the app's icon (dropped in, or
/// changed on another install) becomes the icon. An instance whose folder has
/// no `icon.png` gets its current icon written there; one without any icon
/// gets the icon named in `instance.cfg` or shown by the official Modrinth
/// App.
async fn sync_folder_icons(
    instances_dir: &Path,
    modrinth_app: &HashMap<String, ModrinthAppInstance>,
    state: &State,
) {
    let rows = match instance_rows::list_instances(&state.pool).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!("Could not list instances for icons: {error}");
            return;
        }
    };

    let icon_cache = state.directories.caches_dir().join("icons");
    for row in &rows {
        let dir = instances_dir.join(&row.path);
        if !dir.is_dir() {
            continue;
        }
        let folder_icon = dir.join(FOLDER_ICON);
        let stored = row.icon_path.as_deref().filter(|icon| !icon.is_empty());
        let stored_is_local =
            stored.is_some_and(|icon| Path::new(icon).is_file());
        // A cached icon stored with another install's path (like the other
        // OS of a dual boot) is the same file in this install's cache.
        let current = if stored_is_local {
            stored.map(PathBuf::from)
        } else {
            stored
                .and_then(cached_icon_name)
                .map(|name| icon_cache.join(name))
                .filter(|path| path.is_file())
        };

        let source = if folder_icon.is_file() {
            match &current {
                Some(current)
                    if same_file_content(current, &folder_icon).await =>
                {
                    if stored_is_local {
                        continue;
                    }
                    Some(IconSource::Cached(current.clone()))
                }
                _ => Some(IconSource::File(folder_icon)),
            }
        } else if let Some(current) = current {
            if !stored_is_local {
                Some(IconSource::Cached(current))
            } else {
                if let Err(error) =
                    write_folder_icon(&dir, Some(&current)).await
                {
                    tracing::warn!(
                        "Could not save the icon into {:?}: {error}",
                        row.path
                    );
                }
                continue;
            }
        } else {
            let from_cfg = match read_instance_cfg(&dir).await {
                Ok(CfgRead::Parsed(cfg)) => cfg
                    .icon
                    .as_deref()
                    .map(|name| icon_cache.join(name))
                    .filter(|path| path.is_file()),
                _ => None,
            };
            match from_cfg.map(IconSource::Cached).or_else(|| {
                modrinth_app
                    .get(&row.path)
                    .and_then(|instance| instance.icon.clone())
                    .map(IconSource::File)
            }) {
                Some(source) => Some(source),
                None => linked_project_icon(&row.id, state).await,
            }
        };
        let Some(source) = source else {
            continue;
        };

        let result = async {
            let cached = match &source {
                IconSource::Cached(path) => path.clone(),
                IconSource::File(path) => {
                    crate::api::instance::cache_icon_from_path(path, state)
                        .await?
                }
                IconSource::Url(url) => {
                    let bytes = crate::util::fetch::fetch(
                        url,
                        None,
                        None,
                        None,
                        &state.fetch_semaphore,
                        &state.pool,
                    )
                    .await?;
                    crate::api::instance::cache_icon(bytes, state).await?
                }
            };
            // Saving the icon also writes it into the folder (see
            // `sync_instance_cfg`).
            super::edit_instance::edit_instance(
                &row.id,
                EditInstance {
                    icon_path: Some(Some(cached.to_string_lossy().to_string())),
                    ..EditInstance::default()
                },
                &state.pool,
            )
            .await?;
            emit_instance(&row.id, InstancePayloadType::Edited).await
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(
                "Could not update the icon of {:?}: {error}",
                row.path
            );
        }
    }
}

enum IconSource {
    /// Already in the app's icon cache.
    Cached(PathBuf),
    /// Any image, cached (and normalized) first.
    File(PathBuf),
    /// The icon of the Modrinth project the instance was installed from.
    Url(String),
}

/// The icon of the modpack (or server project) an instance is linked to, for
/// instances without an icon of their own.
async fn linked_project_icon(
    instance_id: &str,
    state: &State,
) -> Option<IconSource> {
    let link = instance_rows::get_instance_link(instance_id, &state.pool)
        .await
        .ok()?;
    let project_id = match link {
        InstanceLink::ModrinthModpack { project_id, .. }
        | InstanceLink::ServerProject { project_id } => project_id,
        InstanceLink::ServerProjectModpack {
            content_project_id, ..
        } => content_project_id,
        InstanceLink::SharedInstance {
            modpack_project_id, ..
        } => modpack_project_id?,
        _ => return None,
    };
    let project = crate::state::CachedEntry::get_project(
        &project_id,
        None,
        &state.pool,
        &state.api_semaphore,
    )
    .await
    .map_err(|error| {
        tracing::warn!("Could not look up project {project_id}: {error}");
    })
    .ok()??;
    project.icon_url.map(IconSource::Url)
}

async fn same_file_content(a: &Path, b: &Path) -> bool {
    match (tokio::fs::read(a).await, tokio::fs::read(b).await) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Makes the folder's `icon.png` match the instance's icon: a copy of it, or
/// no file when the instance has no icon.
async fn write_folder_icon(
    dir: &Path,
    icon: Option<&Path>,
) -> crate::Result<()> {
    let target = dir.join(FOLDER_ICON);
    match icon {
        Some(icon) => {
            if same_file_content(icon, &target).await {
                return Ok(());
            }
            let temporary = dir.join(format!("{FOLDER_ICON}.tmp"));
            crate::util::io::copy(icon, &temporary).await?;
            crate::util::io::rename_or_move(&temporary, &target).await?;
        }
        None => {
            if target.is_file() {
                crate::util::io::remove_file(&target).await?;
            }
        }
    }
    Ok(())
}

/// Turns off queueing installs for scanned instances; the end-to-end test
/// uses it to stay offline and keep instances unlocked.
pub(crate) static QUEUE_INSTALLS: AtomicBool = AtomicBool::new(true);

/// Queues the Minecraft and loader install for instances the scan left
/// `NotInstalled` (new imports, or a version changed in `instance.cfg`), so
/// they can be played without clicking Repair. This only downloads the game
/// into the shared metadata folder; the instance's own files are untouched.
fn queue_minecraft_installs(report: &InstanceScanReport) {
    if !QUEUE_INSTALLS.load(Ordering::Relaxed) {
        return;
    }
    let ids: Vec<String> = report
        .imported
        .iter()
        .chain(&report.relocated)
        .chain(&report.updated_from_cfg)
        .cloned()
        .collect();
    if ids.is_empty() {
        return;
    }

    // Spawned, because the scan also runs while the app is starting up.
    tokio::spawn(async move {
        let Ok(state) = State::get().await else {
            return;
        };
        for id in ids {
            match crate::state::get_instance(&id, &state.pool).await {
                Ok(Some(instance))
                    if instance.instance.install_stage
                        == InstanceInstallStage::NotInstalled
                        && !instance.quarantined => {}
                Ok(_) => continue,
                Err(error) => {
                    tracing::warn!(
                        "Could not load scanned instance {id}: {error}"
                    );
                    continue;
                }
            }
            if let Err(error) =
                crate::install::runner::install_existing_instance(
                    id.clone(),
                    false,
                )
                .await
            {
                tracing::warn!(
                    "Could not queue the install of scanned instance {id}: {error}"
                );
            }
        }
    });
}

/// Tells the frontend about instances the scan added or changed, so they show
/// up without restarting the app.
async fn emit_scan_events(report: &InstanceScanReport) {
    let events = report
        .imported
        .iter()
        .chain(&report.relocated)
        .map(|id| (id, InstancePayloadType::Created))
        .chain(
            report
                .updated_from_cfg
                .iter()
                .map(|id| (id, InstancePayloadType::Edited)),
        );

    for (id, event) in events {
        if let Err(error) = emit_instance(id, event).await {
            tracing::warn!("Could not announce scanned instance {id}: {error}");
        }
    }
}

/// Keeps a folder that already has a database row in sync with its
/// `instance.cfg`: a newer file (for example written by another install
/// sharing the same folder) updates the row, then the file is refreshed from
/// the row.
async fn sync_known_folder(
    row: &Instance,
    dir: &Path,
    pool: &SqlitePool,
    report: &mut InstanceScanReport,
) -> crate::Result<()> {
    if let CfgRead::Parsed(cfg) = read_instance_cfg(dir).await?
        && cfg.modified.is_some_and(|modified| {
            modified.timestamp() > row.modified.timestamp()
        })
        && apply_cfg_to_row(row, &cfg, pool).await?
    {
        report.updated_from_cfg.push(row.id.clone());
    }

    write_cfg_for_instance(&row.id, dir, pool, false).await
}

async fn import_unknown_folder(
    folder: &str,
    dir: &Path,
    instances_dir: &Path,
    modrinth_app: &HashMap<String, ModrinthAppInstance>,
    rows_by_id: &mut HashMap<String, Instance>,
    state: &State,
    report: &mut InstanceScanReport,
) -> crate::Result<()> {
    let cfg = match read_instance_cfg(dir).await? {
        CfgRead::Parsed(cfg) => *cfg,
        read => {
            if is_nested_prism_instance(dir) {
                report.skipped.push((
                    folder.to_string(),
                    "Prism/MultiMC instance layout (.minecraft subfolder) is not supported".to_string(),
                ));
                return Ok(());
            }
            if let Some(instance) = modrinth_app.get(folder) {
                instance.cfg.clone()
            } else {
                if matches!(read, CfgRead::Missing) && !looks_like_instance(dir)
                {
                    return Ok(());
                }
                let dir_owned = dir.to_path_buf();
                let name = folder.to_string();
                let inferred = tokio::task::spawn_blocking(move || {
                    infer_instance_cfg(&dir_owned, &name)
                })
                .await
                .ok()
                .flatten();
                match inferred {
                    Some(cfg) => cfg,
                    None => {
                        report.skipped.push((
                        folder.to_string(),
                        format!(
                            "Minecraft version could not be detected (no Modrinth App entry, world, log or pinned mods); add {}={} to its {}",
                            "ModrinthGameVersion",
                            "<version>",
                            instance_cfg::INSTANCE_CFG_FILE_NAME,
                        ),
                    ));
                        return Ok(());
                    }
                }
            }
        }
    };

    // A folder carrying the id of an instance whose folder no longer exists
    // was renamed or moved while the app was closed: point the row at it.
    if let Some(id) = &cfg.id
        && let Some(row) = rows_by_id.get(id)
        && !instances_dir.join(&row.path).exists()
    {
        let previous_path = row.path.clone();
        relocate_instance(id, folder, &state.pool).await?;
        if let Some(row) = rows_by_id.get_mut(id) {
            row.path = folder.to_string();
        }
        tracing::info!(
            "Instance {id} moved from {previous_path:?} to {folder:?}"
        );
        crate::state::instances::watcher::watch_instance_folder(
            id,
            folder,
            &state.file_watcher,
            &state.directories,
        )
        .await;
        write_cfg_for_instance(id, dir, &state.pool, false).await?;
        report.relocated.push(id.clone());
        return Ok(());
    }

    let id = cfg
        .id
        .clone()
        .filter(|id| id.starts_with("local:") && !rows_by_id.contains_key(id))
        .unwrap_or_else(|| format!("local:{}", Uuid::new_v4()));
    let instance =
        insert_imported_instance(&id, folder, &cfg, &state.pool).await?;
    rows_by_id.insert(id.clone(), instance);
    tracing::info!("Imported instance folder {folder:?} as {id}");

    crate::state::instances::watcher::watch_instance_folder(
        &id,
        folder,
        &state.file_watcher,
        &state.directories,
    )
    .await;
    write_cfg_for_instance(&id, dir, &state.pool, false).await?;
    report.imported.push(id.clone());

    Ok(())
}

async fn insert_imported_instance(
    id: &str,
    folder: &str,
    cfg: &InstanceCfg,
    pool: &SqlitePool,
) -> crate::Result<Instance> {
    let now = Utc::now();
    let content_set_id = format!("content-set:{}", Uuid::new_v4());
    let link = importable_link(cfg.link.clone());
    let name = if cfg.name.trim().is_empty() {
        folder.to_string()
    } else {
        cfg.name.clone()
    };
    let instance = Instance {
        id: id.to_string(),
        path: folder.to_string(),
        applied_content_set_id: Some(content_set_id.clone()),
        // Game files for this OS may be missing; the app offers to install
        // them, which never touches the instance's own content.
        install_stage: InstanceInstallStage::NotInstalled,
        launcher_feature_version: LauncherFeatureVersion::MOST_RECENT,
        update_channel: ReleaseChannel::Release,
        name,
        icon_path: None,
        created: cfg.created.unwrap_or(now),
        modified: now,
        last_played: cfg.last_played,
        // Unsubmitted playtime is left for the install that recorded it to
        // report, so it is never counted twice.
        submitted_time_played: cfg
            .submitted_time_played
            .saturating_add(cfg.recent_time_played),
        recent_time_played: 0,
    };
    let content_set = ContentSet {
        id: content_set_id,
        instance_id: id.to_string(),
        name: "Default".to_string(),
        source_kind: super::create_instance::content_source_kind(&link),
        status: ContentSetStatus::Available,
        game_version: cfg.game_version.clone(),
        protocol_version: None,
        loader: cfg.loader,
        loader_version: cfg.loader_version.clone(),
        created: now,
        modified: now,
    };

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    instance_rows::insert_instance(&instance, &mut tx).await?;
    // Option syncing is off for imported folders so their existing
    // options.txt, servers and hotbars are never overwritten.
    sqlx::query(
        "INSERT INTO instance_sync_preferences (instance_id, feature, enabled)
		SELECT ?, feature, 0 FROM sync_feature_settings",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    content_rows::insert_content_set(&content_set, &mut tx).await?;
    instance_rows::upsert_instance_link(id, &link, &mut tx).await?;
    instance_rows::replace_instance_groups(id, &[], &mut tx).await?;
    instance_rows::upsert_instance_launch_overrides(
        &InstanceLaunchOverrides::empty(id.to_string()),
        &mut tx,
    )
    .await?;
    tx.commit().await?;

    Ok(instance)
}

/// Links that depend on account-bound state (shared instances, hosting) can
/// not be carried over from another install.
fn importable_link(link: Option<InstanceLink>) -> InstanceLink {
    match link {
        Some(
            link @ (InstanceLink::ModrinthModpack { .. }
            | InstanceLink::ImportedModpack { .. }
            | InstanceLink::ServerProject { .. }
            | InstanceLink::ServerProjectModpack { .. }),
        ) => link,
        _ => InstanceLink::Unmanaged,
    }
}

async fn relocate_instance(
    id: &str,
    folder: &str,
    pool: &SqlitePool,
) -> crate::Result<()> {
    sqlx::query("UPDATE instances SET path = ?, modified = ? WHERE id = ?")
        .bind(folder)
        .bind(Utc::now().timestamp())
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Applies a newer `instance.cfg` to an existing row. Returns whether
/// anything changed.
async fn apply_cfg_to_row(
    row: &Instance,
    cfg: &InstanceCfg,
    pool: &SqlitePool,
) -> crate::Result<bool> {
    let Some(content_set) =
        content_rows::get_applied_content_set(&row.id, pool).await?
    else {
        return Ok(false);
    };
    let mut patch = EditInstance::default();

    if !cfg.name.trim().is_empty() && cfg.name != row.name {
        patch.name = Some(cfg.name.clone());
    }
    if cfg.last_played > row.last_played {
        patch.last_played = Some(cfg.last_played);
    }
    // Playtime only ever grows; keep this install's unsubmitted time as is.
    let cfg_total = cfg
        .submitted_time_played
        .saturating_add(cfg.recent_time_played);
    let row_total = row
        .submitted_time_played
        .saturating_add(row.recent_time_played);
    if cfg_total > row_total {
        patch.submitted_time_played =
            Some(cfg_total.saturating_sub(row.recent_time_played));
    }

    let version_changed = cfg.game_version != content_set.game_version
        || cfg.loader != content_set.loader
        || (cfg.loader_version.is_some()
            && cfg.loader_version != content_set.loader_version);
    if version_changed {
        patch.content_set_patch = Some(AppliedContentSetPatch {
            game_version: Some(cfg.game_version.clone()),
            loader: Some(cfg.loader),
            loader_version: cfg.loader_version.clone().map(Some).or(Some(None)),
            ..AppliedContentSetPatch::default()
        });
        // The game files for the new version still need to be installed.
        patch.install_stage = Some(InstanceInstallStage::NotInstalled);
    }

    if patch.name.is_none()
        && patch.last_played.is_none()
        && patch.submitted_time_played.is_none()
        && patch.content_set_patch.is_none()
    {
        return Ok(false);
    }

    super::edit_instance::edit_instance_row(&row.id, patch, pool).await?;
    Ok(true)
}

/// `icon_changed` says the row's icon was just set or removed in the app;
/// otherwise a row without an icon keeps the icon `instance.cfg` names, since
/// rows imported on another install (or before the icon was synced) start
/// without one.
async fn cfg_from_row(
    instance_id: &str,
    dir: &Path,
    pool: &SqlitePool,
    icon_changed: bool,
) -> crate::Result<Option<InstanceCfg>> {
    let Some(instance) =
        instance_rows::get_instance_by_id(instance_id, pool).await?
    else {
        return Ok(None);
    };
    let Some(content_set) =
        content_rows::get_applied_content_set(instance_id, pool).await?
    else {
        return Ok(None);
    };
    let link = instance_rows::get_instance_link(instance_id, pool).await?;

    Ok(Some(InstanceCfg {
        id: Some(instance.id),
        name: instance.name,
        game_version: content_set.game_version,
        loader: content_set.loader,
        loader_version: content_set.loader_version,
        created: Some(instance.created),
        modified: Some(instance.modified),
        last_played: instance.last_played,
        submitted_time_played: instance.submitted_time_played,
        recent_time_played: instance.recent_time_played,
        link: Some(link),
        icon: match instance.icon_path.as_deref() {
            Some(icon) if Path::new(icon).is_file() => cached_icon_name(icon),
            // A path from another install sharing the folder (for example the
            // other OS of a dual boot): keep the icon the file already names.
            Some(icon) if !icon.is_empty() => existing_cfg_icon(dir).await?,
            _ if !icon_changed => existing_cfg_icon(dir).await?,
            _ => None,
        },
    }))
}

async fn existing_cfg_icon(dir: &Path) -> crate::Result<Option<String>> {
    Ok(match read_instance_cfg(dir).await? {
        CfgRead::Parsed(existing) => existing.icon,
        _ => None,
    })
}

/// The file name of an icon in the app's icon cache (`caches/icons`), which
/// every install sharing the app folder can resolve. Icons elsewhere are not
/// recorded. Both separators count, so a path written by Windows is
/// understood on Linux and the other way round.
fn cached_icon_name(icon_path: &str) -> Option<String> {
    let mut parts = icon_path.rsplit(['/', '\\']);
    let name = parts.next().filter(|name| !name.is_empty())?;
    (parts.next() == Some("icons")).then(|| name.to_string())
}

async fn write_cfg_for_instance(
    instance_id: &str,
    dir: &Path,
    pool: &SqlitePool,
    icon_changed: bool,
) -> crate::Result<()> {
    // The file always carries the id of the row it belongs to. Installs
    // sharing a folder agree on it (imports reuse the id from the file), and
    // a copied folder stops pointing at the instance it was copied from.
    let Some(cfg) = cfg_from_row(instance_id, dir, pool, icon_changed).await?
    else {
        return Ok(());
    };
    write_instance_cfg(dir, &cfg).await?;

    Ok(())
}

/// Refreshes an instance's `instance.cfg` after its row changed, and its
/// folder's `icon.png` when `icon_changed` (the icon was set or removed).
/// Failures are logged and never fail the change itself.
pub(crate) async fn sync_instance_cfg(
    instance_id: &str,
    pool: &SqlitePool,
    icon_changed: bool,
) {
    let Some(directories) = DirectoryInfo::global_handle_if_ready() else {
        return;
    };
    let result: crate::Result<()> = async {
        let Some(path) =
            instance_rows::get_instance_path_by_id(instance_id, pool).await?
        else {
            return Ok(());
        };
        let dir = directories.instances_dir().join(path);
        if !dir.is_dir() {
            return Ok(());
        }
        write_cfg_for_instance(instance_id, &dir, pool, icon_changed).await?;

        // The icon lives in the folder too, so a change made in the app
        // reaches every install using the folder. Other edits (like the
        // installs of a modpack) leave the folder's icon alone.
        if icon_changed
            && let Some(instance) =
                instance_rows::get_instance_by_id(instance_id, pool).await?
        {
            match instance
                .icon_path
                .as_deref()
                .filter(|icon| !icon.is_empty())
            {
                None => write_folder_icon(&dir, None).await?,
                Some(icon) if Path::new(icon).is_file() => {
                    write_folder_icon(&dir, Some(Path::new(icon))).await?;
                }
                // An icon path from another install: leave the folder alone.
                Some(_) => {}
            }
        }
        Ok(())
    }
    .await;

    if let Err(error) = result {
        tracing::warn!(
            "Could not update instance.cfg for instance {instance_id}: {error}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::cached_icon_name;

    #[test]
    fn cached_icon_names_from_any_os() {
        assert_eq!(
            cached_icon_name("/mnt/drive/Modrinth/caches/icons/abc.png"),
            Some("abc.png".to_string())
        );
        assert_eq!(
            cached_icon_name(r"E:\Modrinth\caches\icons\abc.png"),
            Some("abc.png".to_string())
        );
        assert_eq!(cached_icon_name("/home/me/Pictures/abc.png"), None);
        assert_eq!(cached_icon_name("/caches/icons/"), None);
    }
}
