use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use gauges_shared::HealthResponse;

use super::{AppState, blocking_database};

pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let healthy = blocking_database(state.database.clone(), |database| Ok(database.is_healthy()))
        .await
        .unwrap_or(false);
    let response = Json(HealthResponse {
        status: if healthy { "ok" } else { "unhealthy" }.into(),
    });
    if healthy {
        (StatusCode::OK, response)
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, response)
    }
}
