use crate::State;
use crate::state::LauncherFeatureVersion;

use super::edit_instance::EditInstance;
use super::scan_instances::InstanceScanReport;

/// Refreshes every instance, starting with a scan of the instances folder.
/// Scan failures are logged so the rest of the refresh still runs.
pub(crate) async fn refresh_all_instances() -> crate::Result<()> {
    let state = State::get().await?;

    // Pick up instance folders added, renamed or changed on disk (for example
    // by another install sharing the instances folder) before refreshing.
    if let Err(error) = super::scan_instances_folder(&state).await {
        tracing::error!("Failed to scan the instances folder: {error}");
    }

    upgrade_launcher_feature_versions(&state).await
}

/// Refreshes every instance on request, returning what the folder scan found.
pub(crate) async fn refresh_instances_with_report()
-> crate::Result<InstanceScanReport> {
    let state = State::get().await?;
    let report = super::scan_instances_folder(&state).await?;
    upgrade_launcher_feature_versions(&state).await?;

    Ok(report)
}

async fn upgrade_launcher_feature_versions(state: &State) -> crate::Result<()> {
    let instances = crate::state::instances::adapters::sqlite::instance_rows::list_instances(
		&state.pool,
	)
	.await?;

    for instance in instances {
        let launcher_feature_version = (instance.launcher_feature_version
            < LauncherFeatureVersion::MOST_RECENT)
            .then_some(LauncherFeatureVersion::MOST_RECENT);

        if launcher_feature_version.is_none() {
            continue;
        }

        super::edit_instance::edit_instance(
            &instance.id,
            EditInstance {
                install_stage: None,
                launcher_feature_version,
                ..EditInstance::default()
            },
            &state.pool,
        )
        .await?;
    }

    Ok(())
}
