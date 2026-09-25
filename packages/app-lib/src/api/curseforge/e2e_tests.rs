//! End-to-end steps for the CurseForge tab, run from the launcher end-to-end
//! test (`scan_instances_e2e_tests`), which boots the real launcher state.

use crate::api::instance as api;
use crate::install::InstallJobStatus;
use crate::state::ModLoader;
use std::time::Duration;

/// FTB Unstable 1.20: Fabric 1.0.0, one of the smallest Feed the Beast packs
/// on a current Minecraft version: 141 client files (118 mods) and one
/// server-only file.
const FTB_PACK: u32 = 109;
const FTB_VERSION: u32 = 6508;

/// Set `THREADRINTH_E2E_SKIP_GAME_INSTALL` where Minecraft's own downloads
/// can't be reached (some sandboxes block the GitHub-hosted LWJGL natives):
/// modpack installs then stop after the pack's files. CI installs the game.
pub(crate) fn skip_game_install() -> bool {
    std::env::var_os("THREADRINTH_E2E_SKIP_GAME_INSTALL").is_some()
}

/// Installs a real Feed the Beast pack as a new instance through the
/// install job, like the CurseForge tab does, and checks the instance.
pub(crate) async fn install_ftb_pack() {
    let pack = crate::api::ftb::get_pack(FTB_PACK).await.unwrap();
    let version = pack
        .versions
        .iter()
        .find(|x| x.id == FTB_VERSION)
        .expect("the pack version");
    assert_eq!(version.game_version.as_deref(), Some("1.20.1"));
    assert_eq!(version.loader.as_deref(), Some("fabric"));

    let report = crate::api::ftb::install_pack(FTB_PACK, FTB_VERSION)
        .await
        .unwrap();
    assert!(report.manual_downloads.is_empty(), "{report:?}");
    assert_eq!(report.instance_name, pack.name);
    let job_id = report.job.job_id.parse().unwrap();

    let mut job = report.job;
    for _ in 0..(20 * 60) {
        job = crate::install::runner::get_job(job_id).await.unwrap();
        if !matches!(
            job.status,
            InstallJobStatus::Queued | InstallJobStatus::Running
        ) {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert_eq!(job.status, InstallJobStatus::Succeeded, "{job:?}");

    let instance_id = job.instance_id.expect("an instance");
    let metadata = api::get(&instance_id).await.unwrap().unwrap();
    assert_eq!(metadata.instance.name, pack.name);
    assert_eq!(metadata.applied_content_set.game_version, "1.20.1");
    assert_eq!(metadata.applied_content_set.loader, ModLoader::Fabric);
    assert_eq!(
        metadata.applied_content_set.loader_version.as_deref(),
        Some("0.14.21")
    );

    let dir = api::get_full_path(&instance_id).await.unwrap();
    let jars = std::fs::read_dir(dir.join("mods"))
        .unwrap()
        .filter_map(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().to_string();
            name.ends_with(".jar").then_some(name)
        })
        .count();
    assert!(jars >= 110, "{jars} mods");
    assert!(dir.join("config").is_dir(), "configs are in");

    assert!(
        matches!(
            metadata.link,
            crate::state::InstanceLink::ImportedModpack { .. }
        ),
        "{:?}",
        metadata.link
    );

    // The pack's files are its content, like an imported .mrpack's.
    api::sync_content_files(&instance_id).await.unwrap();
    let items = api::get_linked_modpack_content(&instance_id, None)
        .await
        .unwrap();
    assert!(items.len() >= 110, "{} content items", items.len());
    println!(
        "FTB pack install: ok ({jars} mods, {} content items)",
        items.len()
    );
}
