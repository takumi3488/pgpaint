use std::{fmt, sync::Arc};

use crate::domain::connection_profile::ConnectionProfileId;
use crate::domain::database::{LintIssue, OwnerId, QueryId, QueryResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryRunStatus {
    Running,
    Succeeded,
    Failed,
    Canceled,
}

#[derive(Debug, Clone)]
pub struct QueryRun {
    pub id: QueryId,
    pub owner: OwnerId,
    pub profile_id: ConnectionProfileId,
    pub sql: String,
    pub warnings: Vec<LintIssue>,
    pub status: QueryRunStatus,
    pub result: Option<QueryResult>,
    pub error: Option<String>,
    pub elapsed_ms: u64,
    pub cancel_requested: bool,
}

#[derive(Debug, Clone)]
pub enum QueryRunOutcome {
    Succeeded(QueryResult),
    Failed(String),
    Canceled(Option<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryRunStoreError {
    Full,
    RunTooLarge,
}

impl fmt::Display for QueryRunStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => formatter
                .write_str("Too many active query runs. Wait for a run to finish and try again."),
            Self::RunTooLarge => {
                formatter.write_str("This query is too large to retain in the bounded run store.")
            }
        }
    }
}

impl std::error::Error for QueryRunStoreError {}

/// Owner- and profile-scoped storage for query runs and their bounded results.
pub trait QueryRunStore: Send + Sync {
    /// # Errors
    /// Returns [`QueryRunStoreError`] when the bounded store cannot retain the run.
    fn insert(&self, run: QueryRun) -> Result<(), QueryRunStoreError>;

    fn get(
        &self,
        owner: &OwnerId,
        profile_id: &ConnectionProfileId,
        id: QueryId,
    ) -> Option<Arc<QueryRun>>;

    /// Marks a still-running query so a failed execution can be recorded as canceled.
    fn request_cancel(
        &self,
        owner: &OwnerId,
        profile_id: &ConnectionProfileId,
        id: QueryId,
    ) -> bool;

    fn clear_cancel_request(&self, owner: &OwnerId, profile_id: &ConnectionProfileId, id: QueryId);

    /// Stores a terminal state and returns the owner-scoped final snapshot.
    fn finish(
        &self,
        owner: &OwnerId,
        profile_id: &ConnectionProfileId,
        id: QueryId,
        outcome: QueryRunOutcome,
    ) -> Option<Arc<QueryRun>>;
}
