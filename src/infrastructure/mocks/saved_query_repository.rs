use crate::domain::connection_profile::ConnectionProfileId;
use crate::domain::saved_query::{SavedQuery, SavedQueryId, SavedQueryRepository};
use async_trait::async_trait;

pub struct MockSavedQueryRepository {}

impl MockSavedQueryRepository {
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for MockSavedQueryRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SavedQueryRepository for MockSavedQueryRepository {
    async fn list(
        &self,
        _owner: Option<&str>,
        _profile_id: Option<&ConnectionProfileId>,
        _limit: u32,
    ) -> Result<Vec<SavedQuery>, gluon::AppError> {
        Ok(vec![])
    }

    async fn list_search(
        &self,
        _owner: Option<&str>,
        _profile_id: Option<&ConnectionProfileId>,
        _limit: u32,
        _search: Option<&str>,
    ) -> Result<Vec<SavedQuery>, gluon::AppError> {
        Ok(vec![])
    }

    async fn find(
        &self,
        _owner: Option<&str>,
        _id: &SavedQueryId,
    ) -> Result<Option<SavedQuery>, gluon::AppError> {
        Ok(None)
    }

    async fn save(&self, _entity: &SavedQuery) -> Result<(), gluon::AppError> {
        Ok(())
    }

    async fn delete(
        &self,
        _owner: Option<&str>,
        _id: &SavedQueryId,
    ) -> Result<(), gluon::AppError> {
        Ok(())
    }
}
