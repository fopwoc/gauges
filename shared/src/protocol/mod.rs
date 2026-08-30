mod devices;
mod history;
mod identity;
mod ingest;
mod metrics;
mod response;
mod validation;

pub use devices::*;
pub use history::*;
pub use identity::*;
pub use ingest::*;
pub use metrics::*;
pub use response::*;
pub use validation::*;

pub const DEVICE_ID_HEADER: &str = "X-Gauges-Device-Id";
pub const INGEST_PATH: &str = "/api/v1/ingest";
pub const MAX_SYNC_MACHINES: usize = 1_000;
pub const MILLIS_PER_HOUR: u64 = 3_600_000;
