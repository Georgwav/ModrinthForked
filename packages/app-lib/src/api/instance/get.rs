use crate::state::{InstanceMetadata, InstanceScanReport, State};

#[tracing::instrument]
pub async fn get(instance_id: &str) -> crate::Result<Option<InstanceMetadata>> {
    let state = State::get().await?;
    crate::state::get_instance(instance_id, &state.pool).await
}

#[tracing::instrument]
pub async fn get_many(
    instance_ids: &[&str],
) -> crate::Result<Vec<InstanceMetadata>> {
    let state = State::get().await?;
    crate::state::get_instances_metadata(instance_ids, &state.pool).await
}

#[tracing::instrument]
pub async fn list() -> crate::Result<Vec<InstanceMetadata>> {
    let state = State::get().await?;
    crate::state::list_instances(&state.pool).await
}

/// Refreshes the instance list: scans the instances folder, importing
/// instance folders the app does not know about yet and refreshing every
/// instance's `instance.cfg`.
#[tracing::instrument]
pub async fn refresh() -> crate::Result<InstanceScanReport> {
    crate::state::refresh_instances_with_report().await
}
