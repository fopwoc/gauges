use std::collections::{BTreeMap, HashSet};

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use gauges_shared::{
    DeviceIndexResponse, DeviceSummary, DeviceSummarySyncRequest, DeviceSummarySyncResponse,
};
use sha2::{Digest, Sha256};

use super::{
    AppState, blocking_database,
    error::{ApiError, json_payload},
};

pub async fn index(State(state): State<AppState>) -> Result<Json<DeviceIndexResponse>, ApiError> {
    let mut devices = state.config.devices.values().collect::<Vec<_>>();
    devices.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    let fingerprint = devices
        .iter()
        .map(|device| format!("{}\u{1}{}", device.id, device.display_name))
        .collect::<Vec<_>>()
        .join("\0");
    let digest = Sha256::digest(fingerprint.as_bytes());
    let revision = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let device_ids = devices
        .iter()
        .map(|device| device.id.clone())
        .collect::<Vec<_>>();
    let database = state.database.clone();
    let database_ids = device_ids.clone();
    let statuses = blocking_database(database, move |database| {
        database.device_statuses(&database_ids)
    })
    .await?;
    let server_time_ms = (state.clock)();
    let minimum_threshold_ms = state.config.online_threshold.as_millis() as i64;
    let online = statuses
        .iter()
        .filter(|status| {
            let cadence_threshold = i64::try_from(status.collection_interval_seconds)
                .unwrap_or(i64::MAX)
                .saturating_mul(2_000);
            let threshold = minimum_threshold_ms.max(cadence_threshold);
            server_time_ms.saturating_sub(status.last_metric_time_ms) <= threshold
        })
        .count();

    Ok(Json(DeviceIndexResponse {
        revision,
        total: devices.len(),
        online,
        offline: devices.len().saturating_sub(online),
        last_metric_time_ms: statuses
            .iter()
            .map(|status| status.last_metric_time_ms)
            .max(),
        device_ids,
    }))
}

pub async fn sync(
    State(state): State<AppState>,
    payload: Result<Json<DeviceSummarySyncRequest>, JsonRejection>,
) -> Result<Json<DeviceSummarySyncResponse>, ApiError> {
    let request = json_payload(payload)?;
    request
        .validate()
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let mut seen = HashSet::new();
    let requested_ids = request
        .device_ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()) && state.config.devices.contains_key(id))
        .collect::<Vec<_>>();
    let server_time_ms = (state.clock)();
    let database = state.database.clone();
    let database_ids = requested_ids.clone();
    let stored = blocking_database(database, move |database| {
        database.devices(&database_ids, server_time_ms)
    })
    .await?
    .into_iter()
    .map(|device| (device.device_id.clone(), device))
    .collect::<BTreeMap<_, _>>();

    let minimum_threshold_ms = state.config.online_threshold.as_millis() as i64;
    let devices = requested_ids
        .into_iter()
        .map(|device_id| {
            let summary = stored.get(&device_id).map(|device| {
                let cadence_threshold = i64::try_from(device.collection_interval_seconds)
                    .unwrap_or(i64::MAX)
                    .saturating_mul(2_000);
                let threshold = minimum_threshold_ms.max(cadence_threshold);
                DeviceSummary {
                    device_id: device.device_id.clone(),
                    display_name: state.config.devices[&device_id].display_name.clone(),
                    identity: device.identity.clone(),
                    retention_hours: device.retention_hours,
                    collection_interval_seconds: device.collection_interval_seconds,
                    last_metric_time_ms: device.last_metric_time_ms,
                    online: server_time_ms.saturating_sub(device.last_metric_time_ms) <= threshold,
                    latest_sample: device.latest_sample.clone(),
                }
            });
            (device_id, summary)
        })
        .collect();
    Ok(Json(DeviceSummarySyncResponse {
        server_time_ms,
        devices,
    }))
}
