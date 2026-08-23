mod devices;
mod error;
mod health;
mod history;
mod ingest;

use std::{sync::Arc, time::SystemTime};

use axum::{Router, extract::DefaultBodyLimit, routing::get};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::{HubConfig, HubDatabase};

pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

#[derive(Clone)]
pub(crate) struct AppState {
    pub config: Arc<HubConfig>,
    pub database: HubDatabase,
    pub clock: Clock,
}

pub fn system_clock() -> i64 {
    SystemTime::UNIX_EPOCH
        .elapsed()
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

pub fn router(config: HubConfig, database: HubDatabase, clock: Clock) -> Router {
    let console_path = config.console_path.clone();
    let console = ServeDir::new(&console_path)
        .precompressed_br()
        .precompressed_gzip()
        .not_found_service(ServeFile::new(console_path.join("index.html")));
    let state = AppState {
        config: Arc::new(config),
        database,
        clock,
    };
    let api = Router::new()
        .route("/health", get(health::health))
        .route("/api/v1/ingest", axum::routing::post(ingest::ingest))
        .route("/api/v1/devices/index", get(devices::index))
        .route("/api/v1/devices/sync", axum::routing::post(devices::sync))
        .route("/api/v1/history/sync", axum::routing::post(history::sync))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .with_state(state);

    api.fallback_service(console)
        .layer(TraceLayer::new_for_http())
}

pub(crate) async fn blocking_database<T, F>(
    database: HubDatabase,
    operation: F,
) -> Result<T, error::ApiError>
where
    T: Send + 'static,
    F: FnOnce(HubDatabase) -> anyhow::Result<T> + Send + 'static,
{
    let result = tokio::task::spawn_blocking(move || operation(database))
        .await
        .map_err(anyhow::Error::from)?;
    result.map_err(error::ApiError::from)
}
