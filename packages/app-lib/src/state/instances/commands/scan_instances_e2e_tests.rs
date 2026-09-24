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
