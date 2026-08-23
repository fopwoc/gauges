use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use gauges_shared::{HistorySyncRequest, HistorySyncResponse, MachineHistory};

use super::{
    AppState, blocking_database,
    error::{ApiError, json_payload},
};

pub async fn sync(
    State(state): State<AppState>,
    payload: Result<Json<HistorySyncRequest>, JsonRejection>,
) -> Result<Json<HistorySyncResponse>, ApiError> {
    let request = json_payload(payload)?;
    request
        .validate()
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let requested = request
        .machines
        .into_iter()
        .filter(|(device_id, _)| state.config.devices.contains_key(device_id))
        .collect::<Vec<_>>();
    let server_time_ms = (state.clock)();
    let database = state.database.clone();
    let machines = blocking_database(database, move |database| {
        requested
            .into_iter()
            .filter_map(|(device_id, cursor)| {
                database
                    .history(&device_id, cursor, server_time_ms)
                    .transpose()
                    .map(|result| result.map(|history| (device_id, history)))
            })
            .collect::<anyhow::Result<BTreeMap<_, _>>>()
    })
    .await?
    .into_iter()
    .map(|(device_id, history)| {
        (
            device_id,
            MachineHistory {
                retention_hours: history.retention_hours,
                collection_interval_seconds: history.collection_interval_seconds,
                next_metric_time_ms: history.next_metric_time_ms,
                samples: history.samples,
            },
        )
    })
    .collect();
    Ok(Json(HistorySyncResponse {
        server_time_ms,
        machines,
    }))
}
