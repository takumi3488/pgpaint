use super::entity::{QueryHistory, QueryHistoryStatus};
use crate::domain::connection_profile::ConnectionProfileId;
use async_trait::async_trait;

#[async_trait]
pub trait QueryHistoryRepository: Send + Sync {
    /// Lists one owner's profile history newest-first with offset paging and optional SQL search.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be read.
    async fn list(
        &self,
        owner: Option<&str>,
        profile_id: &ConnectionProfileId,
        offset: u64,
        limit: u32,
        search: Option<&str>,
    ) -> Result<Vec<QueryHistory>, gluon::AppError>;

    /// Lists an owner's history newest-first with offset paging and optional filters.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be read.
    async fn list_filtered(
        &self,
        owner: Option<&str>,
        profile_id: Option<&ConnectionProfileId>,
        offset: u64,
        limit: u32,
        search: Option<&str>,
        status: Option<QueryHistoryStatus>,
    ) -> Result<Vec<QueryHistory>, gluon::AppError>;

    /// Counts history entries matching the same filters as `list_filtered`.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be read.
    async fn count(
        &self,
        owner: Option<&str>,
        profile_id: Option<&ConnectionProfileId>,
        search: Option<&str>,
        status: Option<QueryHistoryStatus>,
    ) -> Result<u64, gluon::AppError>;

    /// Inserts or updates one history entry.
    ///
    /// # Errors
    /// Returns a conflict if the ID belongs to another owner or profile, or a database error.
    async fn save(&self, entity: &QueryHistory) -> Result<(), gluon::AppError>;

    /// Finds one history entry only when its owner and profile match.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be read.
    async fn find(
        &self,
        owner: Option<&str>,
        profile_id: &ConnectionProfileId,
        id: &super::value_objects::QueryHistoryId,
    ) -> Result<Option<QueryHistory>, gluon::AppError>;

    /// Deletes an owner's history entry, optionally requiring a profile match.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be updated.
    async fn delete(
        &self,
        owner: Option<&str>,
        profile_id: Option<&ConnectionProfileId>,
        id: &super::value_objects::QueryHistoryId,
    ) -> Result<(), gluon::AppError>;

    /// Deletes all but the newest `keep_newest` entries for this owner and profile.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be updated.
    async fn prune(
        &self,
        owner: Option<&str>,
        profile_id: &ConnectionProfileId,
        keep_newest: u32,
    ) -> Result<(), gluon::AppError>;
}
