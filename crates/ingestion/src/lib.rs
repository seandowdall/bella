pub mod credentials;
pub mod openai;

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct SyncOutcome {
    pub sync_run_id: Uuid,
    pub provider_account_id: Uuid,
    pub provider: String,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub usage_buckets: usize,
    pub cost_snapshots: usize,
}
