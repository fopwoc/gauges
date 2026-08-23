use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::HeaderMap,
};
use gauges_shared::{DEVICE_ID_HEADER, IngestRequest, IngestResponse};
use subtle::ConstantTimeEq;

use super::{
    AppState, blocking_database,
    error::{ApiError, json_payload},
};

pub async fn ingest(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<IngestRequest>, JsonRejection>,
) -> Result<Json<IngestResponse>, ApiError> {
    let request = json_payload(payload)?;
    let device_id = headers
        .get(DEVICE_ID_HEADER)
        .and_then(|value| value.to_str().ok());
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token);
    let device = device_id
        .and_then(|device_id| state.config.devices.get(device_id))
        .filter(|device| {
            token.is_some_and(|token| bool::from(token.as_bytes().ct_eq(device.token.as_bytes())))
        })
        .cloned()
        .ok_or(ApiError::Unauthorized)?;
    request
        .validate(state.config.max_ingest_batch_size)
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    let server_time_ms = (state.clock)();
    let database = state.database.clone();
    let accepted_sample_ids = blocking_database(database, move |database| {
        database.ingest(
            &device.id,
            &device.display_name,
            &request.identity,
            request.retention_hours,
            request.collection_interval_seconds,
            &request.samples,
            server_time_ms,
        )
    })
    .await?;
    Ok(Json(IngestResponse {
        accepted_sample_ids,
        server_time_ms,
    }))
}

fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("Bearer").then_some(token)
}
