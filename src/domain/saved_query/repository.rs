use super::entity::SavedQuery;
use super::value_objects::SavedQueryId;
use crate::domain::connection_profile::ConnectionProfileId;
use async_trait::async_trait;

#[async_trait]
pub trait SavedQueryRepository: Send + Sync {
    /// Lists an owner's saved queries, optionally filtered to a profile.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be read.
    async fn list(
        &self,
        owner: Option<&str>,
        profile_id: Option<&ConnectionProfileId>,
        limit: u32,
    ) -> Result<Vec<SavedQuery>, gluon::AppError>;

    /// Lists an owner's saved queries, optionally filtered by profile and text search.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be read.
    async fn list_search(
        &self,
        owner: Option<&str>,
        profile_id: Option<&ConnectionProfileId>,
        limit: u32,
        search: Option<&str>,
    ) -> Result<Vec<SavedQuery>, gluon::AppError>;

    /// Finds a saved query only when its owner matches `owner`.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be read.
    async fn find(
        &self,
        owner: Option<&str>,
        id: &SavedQueryId,
    ) -> Result<Option<SavedQuery>, gluon::AppError>;

    /// Inserts or updates a saved query without allowing ownership transfer.
    ///
    /// # Errors
    /// Returns a conflict if the ID belongs to another owner, or a database error.
    async fn save(&self, entity: &SavedQuery) -> Result<(), gluon::AppError>;

    /// Deletes a saved query only when its owner matches `owner`.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be updated.
    async fn delete(&self, owner: Option<&str>, id: &SavedQueryId) -> Result<(), gluon::AppError>;
}
