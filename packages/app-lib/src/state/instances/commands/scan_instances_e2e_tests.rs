//! End-to-end check that folder-based instances work with the rest of the
//! launcher: importing, content indexing, update checks, updating and
//! disabling mods, renames, copies, edits from another install, and synced
//! options.
//!
//! Boots the real launcher state against a temporary directory and talks to
//! the Modrinth API, so it is ignored by default:
//! `cargo test -p theseus --lib scan_instances_e2e -- --ignored --nocapture`

use crate::api::instance as api;
use crate::state::instances::instance_cfg::{CfgRead, read_instance_cfg};
use crate::state::{
    EditInstance, InstanceSyncedOption, ModLoader, State, instances,
};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const OLD_SODIUM_URL: &str = "https://cdn.modrinth.com/data/AANobbMI/versions/vgceLbdH/sodium-fabric-mc1.20-0.4.10%2Bbuild.27.jar";
const OLD_SODIUM_FILE: &str = "sodium-fabric-mc1.20-0.4.10+build.27.jar";
const SODIUM_PROJECT_ID: &str = "AANobbMI";
/// Iris 1.6.4, which requires the old Sodium above; its updates require newer
/// ones.
const OLD_IRIS_VERSION: &str = "URWeWMAt";
const OPTIONS_TXT: &str = "version:3465\nfov:0.25\nrenderDistance:7\n";

fn write(path: &Path, contents: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// A tiny Fabric mod jar containing only a `fabric.mod.json`.
fn write_fake_fabric_jar(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    zip.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(br#"{"schemaVersion":1,"id":"fake","version":"1.0.0"}"#)
        .unwrap();
    zip.finish().unwrap();
}

/// A gzipped `level.dat` saved by the given Minecraft version.
fn write_level_dat(path: &Path, version: &str) {
    use quartz_nbt::{NbtCompound, NbtTag};
    let mut version_tag = NbtCompound::new();
    version_tag.insert("Name", NbtTag::String(version.to_string()));
    let mut data = NbtCompound::new();
    data.insert("Version", NbtTag::Compound(version_tag));
    data.insert("LevelName", NbtTag::String("World".to_string()));
    let mut root = NbtCompound::new();
    root.insert("Data", NbtTag::Compound(data));
    let mut bytes = Vec::new();
    quartz_nbt::io::write_nbt(
        &mut bytes,
        None,
        &root,
        quartz_nbt::io::Flavor::GzCompressed,
    )
    .unwrap();
    write(path, bytes);
}

async fn download(url: &str) -> Vec<u8> {
    reqwest::get(url)
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .bytes()
        .await
        .unwrap()
        .to_vec()
}

async fn cfg(dir: &Path) -> instances::instance_cfg::InstanceCfg {
    match read_instance_cfg(dir).await.unwrap() {
        CfgRead::Parsed(cfg) => *cfg,
        _ => panic!("{} has no parsed instance.cfg", dir.display()),
    }
}

async fn instance_by_path(path: &str) -> Option<instances::InstanceMetadata> {
    api::list()
        .await
        .unwrap()
        .into_iter()
        .find(|metadata| metadata.instance.path == path)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "boots the launcher and uses the Modrinth API"]
async fn folder_instances_work_with_launcher_features() {
    let root = tempfile::tempdir().unwrap();
    // SAFETY: set before the launcher state reads it; no other test in this
    // binary initializes the launcher state.
    unsafe { std::env::set_var("THESEUS_CONFIG_DIR", root.path()) };
    let profiles: PathBuf = root.path().join("profiles");

    // A Prism-style folder with an instance.cfg, a real (outdated) mod and
    // custom game options.
    let prism = profiles.join("Prism Style");
    write(
        &prism.join("instance.cfg"),
        "[General]\nInstanceType=OneSix\nname=Prism Style\nModrinthGameVersion=1.20.1\nModrinthLoader=fabric\ntotalTimePlayed=3600\n",
    );
    write(
        &prism.join("mods").join(OLD_SODIUM_FILE),
        download(OLD_SODIUM_URL).await,
    );
    write(&prism.join("options.txt"), OPTIONS_TXT);

    // An instance from an old Modrinth App version.
    write(
        &profiles.join("Legacy").join("profile.json"),
        r#"{"path":"Legacy","metadata":{"name":"Legacy","game_version":"1.19.2","loader":"forge","loader_version":{"id":"43.2.0"}}}"#,
    );

    // A plain folder: version from its world, loader from its mods.
    let world_only = profiles.join("World Only");
    write_level_dat(&world_only.join("saves/World/level.dat"), "1.19.4");
    write_fake_fabric_jar(&world_only.join("mods/fake.jar"));

    // Looks like an instance, but its version can't be known: skipped.
    std::fs::create_dir_all(profiles.join("Unknown Version/mods")).unwrap();
    // Not an instance at all: ignored.
    write(
        &profiles.join("Screenshots Backup/readme.txt"),
        "not an instance",
    );

    // Installing Minecraft for the imports would download the game and lock
    // the instances this test edits.
    super::scan_instances::QUEUE_INSTALLS
        .store(false, std::sync::atomic::Ordering::Relaxed);
    State::init("ThreadrinthE2E".to_string()).await.unwrap();

    // --- Import ---------------------------------------------------------
    let prism_meta = instance_by_path("Prism Style").await.expect("imported");
    assert_eq!(prism_meta.applied_content_set.game_version, "1.20.1");
    assert_eq!(prism_meta.applied_content_set.loader, ModLoader::Fabric);
    assert_eq!(prism_meta.instance.submitted_time_played, 3600);
    let legacy = instance_by_path("Legacy").await.expect("imported");
    assert_eq!(legacy.applied_content_set.game_version, "1.19.2");
    assert_eq!(legacy.applied_content_set.loader, ModLoader::Forge);
    assert_eq!(
        legacy.applied_content_set.loader_version.as_deref(),
        Some("43.2.0")
    );
    let world = instance_by_path("World Only").await.expect("imported");
    assert_eq!(world.applied_content_set.game_version, "1.19.4");
    assert_eq!(world.applied_content_set.loader, ModLoader::Fabric);
    assert!(instance_by_path("Unknown Version").await.is_none());
    assert!(instance_by_path("Screenshots Backup").await.is_none());
    assert!(!profiles.join("Unknown Version/instance.cfg").exists());
    assert!(!profiles.join("Screenshots Backup/instance.cfg").exists());
    assert!(
        crate::api::onboarding_checklist::get()
            .await
            .unwrap()
            .has_created_instance,
        "imported instances dismiss the welcome screen"
    );
    println!("import: ok");

    let prism_id = prism_meta.instance.id.clone();
    let prism_cfg = cfg(&prism).await;
    assert_eq!(prism_cfg.id.as_deref(), Some(prism_id.as_str()));
    assert!(cfg(&profiles.join("Legacy")).await.id.is_some());

    // Background startup tasks (synced options reconcile, watchers) must not
    // touch the imported game options.
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        std::fs::read_to_string(prism.join("options.txt")).unwrap(),
        OPTIONS_TXT
    );
    println!("options.txt untouched: ok");

    // --- Content indexing, update check, update, disable -----------------
    api::sync_content_files(&prism_id).await.unwrap();
    let items = api::get_content_items(&prism_id, None).await.unwrap();
    let sodium = items
        .iter()
        .find(|item| item.file_name == OLD_SODIUM_FILE)
        .expect("sodium indexed");
    assert_eq!(
        sodium.project.as_ref().map(|project| project.id.as_str()),
        Some(SODIUM_PROJECT_ID),
        "sodium recognized through the Modrinth API"
    );
    api::refresh_content_updates(&prism_id).await.unwrap();
    let items = api::get_content_items(&prism_id, None).await.unwrap();
    let sodium = items
        .iter()
        .find(|item| item.file_name == OLD_SODIUM_FILE)
        .unwrap();
    assert!(sodium.has_update, "update found for old sodium");
    println!("content index + update check: ok");

    let new_path = api::update_project(&prism_id, &sodium.file_path, None)
        .await
        .unwrap();
    assert!(!prism.join("mods").join(OLD_SODIUM_FILE).exists());
    assert!(prism.join(&new_path).exists(), "updated jar at {new_path}");
    let items = api::get_content_items(&prism_id, None).await.unwrap();
    let updated = items
        .iter()
        .find(|item| item.file_path == new_path)
        .expect("updated sodium listed");
    assert!(!updated.has_update);
    println!("mod update: ok ({new_path})");

    let enabled_of = |items: &[crate::state::ContentItem]| {
        items
            .iter()
            .find(|item| {
                item.file_path.trim_end_matches(".disabled") == new_path
            })
            .map(|item| item.enabled)
    };
    api::toggle_disable_project(&prism_id, &new_path, Some(false))
        .await
        .unwrap();
    assert!(prism.join(format!("{new_path}.disabled")).exists());
    assert!(!prism.join(&new_path).exists());
    let items = api::get_content_items(&prism_id, None).await.unwrap();
    assert_eq!(enabled_of(&items), Some(false));
    let enabled_path =
        api::toggle_disable_project(&prism_id, &new_path, Some(true))
            .await
            .unwrap();
    assert!(prism.join(&enabled_path).exists());
    let items = api::get_content_items(&prism_id, None).await.unwrap();
    assert_eq!(enabled_of(&items), Some(true));
    println!("disable/enable: ok");

    // --- Rename in the app ------------------------------------------------
    api::edit(
        &prism_id,
        EditInstance {
            name: Some("Renamed In App".to_string()),
            ..EditInstance::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(cfg(&prism).await.name, "Renamed In App");
    println!("rename in app -> instance.cfg: ok");

    // --- Folder renamed on disk -------------------------------------------
    let moved = profiles.join("Moved On Disk");
    std::fs::rename(&prism, &moved).unwrap();
    let report = api::refresh().await.unwrap();
    assert_eq!(report.relocated, vec![prism_id.clone()]);
    assert!(report.imported.is_empty());
    let moved_meta = api::get(&prism_id).await.unwrap().unwrap();
    assert_eq!(moved_meta.instance.path, "Moved On Disk");
    assert_eq!(moved_meta.instance.name, "Renamed In App");
    api::sync_content_files(&prism_id).await.unwrap();
    let items = api::get_content_items(&prism_id, None).await.unwrap();
    assert!(items.iter().any(|item| item.file_path == enabled_path));
    println!("folder rename on disk: ok");

    // --- Folder copied on disk, then the copy renamed -----------------------
    let copy = profiles.join("Copy");
    copy_dir(&moved, &copy);
    let report = api::refresh().await.unwrap();
    assert_eq!(report.imported.len(), 1, "copy imported: {report:?}");
    let copy_id = report.imported[0].clone();
    assert_ne!(copy_id, prism_id);
    assert_eq!(cfg(&copy).await.id.as_deref(), Some(copy_id.as_str()));
    let renamed_copy = profiles.join("Copy Renamed");
    std::fs::rename(&copy, &renamed_copy).unwrap();
    let report = api::refresh().await.unwrap();
    assert_eq!(report.relocated, vec![copy_id.clone()], "{report:?}");
    assert!(report.imported.is_empty(), "{report:?}");
    assert_eq!(
        api::get(&copy_id).await.unwrap().unwrap().instance.path,
        "Copy Renamed"
    );
    println!("copy + rename copy: ok");

    // --- Edited by another install sharing the folder ---------------------
    let mut shared = cfg(&moved).await;
    shared.name = "Renamed Elsewhere".to_string();
    shared.submitted_time_played += 600;
    shared.modified = Some(chrono::Utc::now() + chrono::Duration::seconds(5));
    instances::instance_cfg::write_instance_cfg(&moved, &shared)
        .await
        .unwrap();
    let report = api::refresh().await.unwrap();
    assert_eq!(report.updated_from_cfg, vec![prism_id.clone()]);
    let pulled = api::get(&prism_id).await.unwrap().unwrap();
    assert_eq!(pulled.instance.name, "Renamed Elsewhere");
    assert!(pulled.instance.submitted_time_played >= 4200);
    println!("edit from another install: ok");

    // Scanning again changes nothing.
    let report = api::refresh().await.unwrap();
    assert!(!report.changed(), "{report:?}");
    println!("idempotent rescan: ok");

    // --- Instances created in the app, synced options ---------------------
    let created = api::create(
        "Created In App".to_string(),
        "1.20.1".to_string(),
        ModLoader::Vanilla,
        None,
        None,
        None,
        instances::InstanceLink::Unmanaged,
    )
    .await
    .unwrap();
    let created_dir = profiles.join(&created.instance.path);
    assert_eq!(
        cfg(&created_dir).await.id.as_deref(),
        Some(created.instance.id.as_str())
    );
    for id in [&prism_id, &created.instance.id] {
        api::set_synced_option(
            id,
            InstanceSyncedOption::MultiplayerServers,
            true,
            None,
        )
        .await
        .unwrap();
        let meta = api::set_synced_option(
            id,
            InstanceSyncedOption::MultiplayerServers,
            false,
            None,
        )
        .await
        .unwrap();
        assert!(!meta.synced_options.multiplayer_servers);
    }
    println!("created instance + synced options: ok");

    // --- Icons -----------------------------------------------------------
    // A folder's own icon.png becomes the icon of an instance without one.
    let mut icon = Vec::new();
    image::RgbaImage::from_pixel(16, 16, image::Rgba([255, 160, 0, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut icon),
            image::ImageFormat::Png,
        )
        .unwrap();
    write(&profiles.join("Legacy/icon.png"), &icon);
    api::refresh().await.unwrap();
    let legacy = instance_by_path("Legacy").await.unwrap();
    let icon_path = legacy.instance.icon_path.expect("icon from icon.png");
    assert!(Path::new(&icon_path).is_file());
    assert!(profiles.join("Legacy/icon.png").is_file(), "source kept");
    println!("icon from folder: ok");

    // The icon is recorded in instance.cfg, so another install sharing the
    // app folder (e.g. the other OS of a dual boot) finds it again even
    // though the icon path in its own database doesn't exist there.
    api::refresh().await.unwrap();
    let icon_name = Path::new(&icon_path)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        cfg(&profiles.join("Legacy")).await.icon.as_deref(),
        Some(icon_name.as_str())
    );
    std::fs::remove_file(profiles.join("Legacy/icon.png")).unwrap();
    let state = State::get().await.unwrap();
    crate::state::edit_instance(
        &legacy.instance.id,
        EditInstance {
            icon_path: Some(Some(
                "/other-os/caches/icons/missing.png".to_string(),
            )),
            ..EditInstance::default()
        },
        &state.pool,
    )
    .await
    .unwrap();
    api::refresh().await.unwrap();
    let restored = instance_by_path("Legacy")
        .await
        .unwrap()
        .instance
        .icon_path
        .unwrap();
    assert_eq!(Path::new(&restored), Path::new(&icon_path));
    println!("icon from instance.cfg on another install: ok");

    // The folder's icon.png is the source of truth: it was written back into
    // the folder, a new image dropped in replaces the icon, and removing the
    // icon in the app removes the file.
    assert!(
        profiles.join("Legacy/icon.png").is_file(),
        "icon written to folder"
    );
    let mut blue = Vec::new();
    image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 80, 255, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut blue),
            image::ImageFormat::Png,
        )
        .unwrap();
    write(&profiles.join("Legacy/icon.png"), &blue);
    api::refresh().await.unwrap();
    let replaced = instance_by_path("Legacy")
        .await
        .unwrap()
        .instance
        .icon_path
        .unwrap();
    assert_ne!(
        Path::new(&replaced),
        Path::new(&icon_path),
        "folder icon wins"
    );
    assert_eq!(
        std::fs::read(&replaced).unwrap(),
        std::fs::read(profiles.join("Legacy/icon.png")).unwrap()
    );
    crate::state::edit_instance(
        &legacy.instance.id,
        EditInstance {
            icon_path: Some(None),
            ..EditInstance::default()
        },
        &state.pool,
    )
    .await
    .unwrap();
    assert!(
        !profiles.join("Legacy/icon.png").exists(),
        "icon removed from folder"
    );
    api::refresh().await.unwrap();
    assert!(
        instance_by_path("Legacy")
            .await
            .unwrap()
            .instance
            .icon_path
            .is_none()
    );
    println!("folder icon.png is the source of truth: ok");

    // Other edits (like the ones a modpack install makes) keep the folder's
    // icon, even while the row has none yet.
    write(&profiles.join("Legacy/icon.png"), &blue);
    crate::state::edit_instance(
        &legacy.instance.id,
        EditInstance {
            name: Some("Legacy Renamed".to_string()),
            ..EditInstance::default()
        },
        &state.pool,
    )
    .await
    .unwrap();
    assert!(
        profiles.join("Legacy/icon.png").is_file(),
        "an unrelated edit keeps the folder icon"
    );
    api::refresh().await.unwrap();
    assert!(
        instance_by_path("Legacy")
            .await
            .unwrap()
            .instance
            .icon_path
            .is_some(),
        "the folder icon becomes the icon"
    );
    println!("unrelated edits keep the folder icon: ok");

    // A modpack instance without an icon of its own gets the modpack's icon;
    // a custom icon still wins.
    let linked = profiles.join("Linked Pack");
    write(
        &linked.join("instance.cfg"),
        "[General]\nModrinthGameVersion=1.20.1\nModrinthLoader=vanilla\n",
    );
    api::refresh().await.unwrap();
    let linked_id = instance_by_path("Linked Pack")
        .await
        .expect("imported")
        .instance
        .id;
    let mut tx = state.pool.begin().await.unwrap();
    crate::state::instances::adapters::sqlite::instance_rows::upsert_instance_link(
        &linked_id,
        &crate::state::InstanceLink::ModrinthModpack {
            project_id: "1KVo5zza".to_string(),
            version_id: "unknown".to_string(),
        },
        &mut tx,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    api::refresh().await.unwrap();
    let pack_icon = instance_by_path("Linked Pack")
        .await
        .unwrap()
        .instance
        .icon_path
        .expect("the modpack's icon");
    assert!(Path::new(&pack_icon).is_file());
    assert!(linked.join("icon.png").is_file(), "saved into the folder");
    write(&linked.join("icon.png"), &blue);
    api::refresh().await.unwrap();
    let custom = instance_by_path("Linked Pack")
        .await
        .unwrap()
        .instance
        .icon_path
        .unwrap();
    assert_eq!(std::fs::read(&custom).unwrap(), blue, "custom icon wins");
    println!("modpack icon for instances without one: ok");

    // --- Install without Repair ----------------------------------------
    // A new import is queued for install, so Play works right away.
    super::scan_instances::QUEUE_INSTALLS
        .store(true, std::sync::atomic::Ordering::Relaxed);
    write(
        &profiles.join("Auto Install/instance.cfg"),
        "[General]\nModrinthGameVersion=1.20.1\nModrinthLoader=vanilla\n",
    );
    let report = api::refresh().await.unwrap();
    let auto_install =
        instance_by_path("Auto Install").await.expect("imported");
    assert!(report.imported.contains(&auto_install.instance.id));
    let mut job = None;
    for _ in 0..100 {
        job = crate::install::runner::list_jobs(true)
            .await
            .unwrap()
            .into_iter()
            .find(|job| {
                job.instance_id.as_deref() == Some(&auto_install.instance.id)
            });
        if job.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let job = job.expect("install queued for the import");
    let job_id = job.job_id.clone();
    let _ =
        crate::install::runner::cancel_job(job.job_id.parse().unwrap()).await;
    println!("install queued without repair: ok");

    // --- Install jobs stored as binary JSON ------------------------------
    // Older builds stored them that way after moving the app folder, which
    // broke every job list ("invalid utf-8 sequence"); the repair migration
    // makes them readable again.
    let state = State::get().await.unwrap();
    sqlx::query("UPDATE install_jobs SET state = jsonb(state)")
        .execute(&state.pool)
        .await
        .unwrap();
    let error = crate::install::runner::list_jobs(true).await.unwrap_err();
    assert!(error.to_string().contains("utf-8"), "{error}");
    sqlx::raw_sql(include_str!(
        "../../../../migrations/20260925120000_threadrinth-install-jobs-text-state.sql"
    ))
    .execute(&state.pool)
    .await
    .unwrap();
    assert!(
        crate::install::runner::list_jobs(true)
            .await
            .unwrap()
            .iter()
            .any(|job| job.job_id == job_id)
    );
    println!("binary install job states repaired: ok");

    // --- Update all -----------------------------------------------------
    // Sodium dropped into the folder has no content entry; Iris, installed
    // through the app, requires it. Updating both used to also plan Sodium as
    // a new dependency, and the two downloads collided ("The updated filename
    // belongs to another content item").
    super::scan_instances::QUEUE_INSTALLS
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let shaders = profiles.join("Shaders");
    write(
        &shaders.join("instance.cfg"),
        "[General]\nModrinthGameVersion=1.20.1\nModrinthLoader=fabric\n",
    );
    write(
        &shaders.join("mods").join(OLD_SODIUM_FILE),
        &download(OLD_SODIUM_URL).await,
    );
    api::refresh().await.unwrap();
    let shaders_id = instance_by_path("Shaders")
        .await
        .expect("imported")
        .instance
        .id;
    api::sync_content_files(&shaders_id).await.unwrap();
    // Downloads report progress through loading bars.
    #[cfg(not(feature = "tauri"))]
    crate::EventState::init().await.unwrap();
    // Iris installed through the app, so it has a content entry.
    api::add_project_from_version(
        &shaders_id,
        OLD_IRIS_VERSION,
        crate::util::fetch::DownloadReason::Standalone,
        None,
    )
    .await
    .unwrap();
    api::refresh_content_updates(&shaders_id).await.unwrap();
    let updated = api::update_all_projects(&shaders_id).await.unwrap();
    assert_eq!(updated.len(), 2, "both mods updated: {updated:?}");
    let jars = std::fs::read_dir(shaders.join("mods"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        jars.iter().filter(|jar| jar.contains("sodium")).count(),
        1,
        "one Sodium jar: {jars:?}"
    );
    assert!(!jars.iter().any(|jar| jar == OLD_SODIUM_FILE), "{jars:?}");
    println!("update all with a required dependency: ok ({jars:?})");

    // --- Server pack ----------------------------------------------------
    // The instance has no saved Fabric version (it used to fail with
    // "Loader version mismatch"); Iris and Sodium are client-only, so they
    // start unchecked, and configs are in.
    write(&shaders.join("config/server.toml"), "motd = 1\n");
    assert!(
        api::get(&shaders_id)
            .await
            .unwrap()
            .unwrap()
            .applied_content_set
            .loader_version
            .is_none()
    );
    let selection = api::server_pack_selection(&shaders_id).await.unwrap();
    assert!(selection.included.contains(&"mods".to_string()));
    assert!(selection.included.contains(&"config".to_string()));
    assert_eq!(selection.excluded.len(), 2, "{:?}", selection.excluded);
    let export_dir = tempfile::tempdir().unwrap();
    let zip_path = export_dir.path().join("server.zip");
    let report = api::export_server_pack(
        &shaders_id,
        zip_path.clone(),
        selection.included,
        selection.excluded,
    )
    .await
    .unwrap();
    assert_eq!(report.mods_included, 0);
    let mut zip =
        zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    let names = (0..zip.len())
        .map(|index| zip.by_index(index).unwrap().name().to_string())
        .collect::<Vec<_>>();
    for expected in [
        "config/server.toml",
        "fabric-server-launch.jar",
        "start.sh",
        "start.bat",
        "README.txt",
    ] {
        assert!(names.iter().any(|name| name == expected), "{names:?}");
    }
    assert!(
        !names.iter().any(|name| name.starts_with("mods/")),
        "{names:?}"
    );
    assert!(
        zip.by_name("fabric-server-launch.jar").unwrap().size() > 10_000,
        "a real launcher jar"
    );
    println!("server pack: ok ({} files)", names.len());

    // --- Copy and move worlds between instances ----------------------------
    use crate::api::world_transfer::{WorldTransferMode, transfer_world};
    let world_only_id = world.instance.id.clone();
    let singleplayer = |worlds: Vec<crate::api::worlds::World>| {
        worlds
            .into_iter()
            .filter(|world| {
                matches!(
                    world.details,
                    crate::api::worlds::WorldDetails::Singleplayer { .. }
                )
            })
            .count()
    };
    let copied = transfer_world(
        &world_only_id,
        "World",
        &shaders_id,
        WorldTransferMode::Copy,
    )
    .await
    .unwrap();
    assert_eq!(copied, "World");
    let again = transfer_world(
        &world_only_id,
        "World",
        &shaders_id,
        WorldTransferMode::Copy,
    )
    .await
    .unwrap();
    assert_eq!(again, "World (2)", "a copy never overwrites");
    let moved = transfer_world(
        &world_only_id,
        "World",
        &prism_id,
        WorldTransferMode::Move,
    )
    .await
    .unwrap();
    assert_eq!(moved, "World");
    assert_eq!(
        singleplayer(
            crate::api::worlds::get_instance_worlds(&shaders_id)
                .await
                .unwrap()
        ),
        2
    );
    assert_eq!(
        singleplayer(
            crate::api::worlds::get_instance_worlds(&world_only_id)
                .await
                .unwrap()
        ),
        0,
        "moved away"
    );
    assert!(
        singleplayer(
            crate::api::worlds::get_instance_worlds(&prism_id)
                .await
                .unwrap()
        ) >= 1,
        "moved in"
    );
    println!("copy and move worlds: ok");
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
