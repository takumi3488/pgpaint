use super::value_objects::QueryHistoryId;
use crate::domain::connection_profile::ConnectionProfileId;
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryHistoryStatus {
    Succeeded,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryHistory {
    pub id: QueryHistoryId,
    pub owner: Option<String>,
    pub profile_id: ConnectionProfileId,
    pub sql: String,
    pub executed_at: OffsetDateTime,
    pub elapsed_ms: u64,
    pub rows: Option<u64>,
    pub rows_affected: Option<u64>,
    pub status: QueryHistoryStatus,
    pub error_message: Option<String>,
}
