use super::entity::ConnectionProfile;
use super::value_objects::ConnectionProfileId;
use async_trait::async_trait;

#[async_trait]
pub trait ConnectionProfileRepository: Send + Sync {
    /// Lists at most `limit` profiles owned by `owner`.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be read.
    async fn list(
        &self,
        owner: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ConnectionProfile>, gluon::AppError>;
    /// Finds a profile only when its owner matches `owner`.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be read.
    async fn find(
        &self,
        owner: Option<&str>,
        id: &ConnectionProfileId,
    ) -> Result<Option<ConnectionProfile>, gluon::AppError>;
    /// Inserts or updates a profile without allowing ownership transfer.
    ///
    /// # Errors
    /// Returns validation, conflict, or metadata database errors.
    async fn save(&self, entity: &ConnectionProfile) -> Result<(), gluon::AppError>;
    /// Deletes a profile only when its owner matches `owner`.
    ///
    /// # Errors
    /// Returns an internal error if the metadata database cannot be updated.
    async fn delete(
        &self,
        owner: Option<&str>,
        id: &ConnectionProfileId,
    ) -> Result<(), gluon::AppError>;
}
