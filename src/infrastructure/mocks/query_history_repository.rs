use crate::domain::connection_profile::ConnectionProfileId;
use crate::domain::query_history::{
    QueryHistory, QueryHistoryId, QueryHistoryRepository, QueryHistoryStatus,
};
use async_trait::async_trait;

pub struct MockQueryHistoryRepository {}

impl MockQueryHistoryRepository {
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for MockQueryHistoryRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl QueryHistoryRepository for MockQueryHistoryRepository {
    async fn list(
        &self,
        _owner: Option<&str>,
        _profile_id: &ConnectionProfileId,
        _offset: u64,
        _limit: u32,
        _search: Option<&str>,
    ) -> Result<Vec<QueryHistory>, gluon::AppError> {
        Ok(vec![])
    }

    async fn list_filtered(
        &self,
        _owner: Option<&str>,
        _profile_id: Option<&ConnectionProfileId>,
        _offset: u64,
        _limit: u32,
        _search: Option<&str>,
        _status: Option<QueryHistoryStatus>,
    ) -> Result<Vec<QueryHistory>, gluon::AppError> {
        Ok(vec![])
    }

    async fn count(
        &self,
        _owner: Option<&str>,
        _profile_id: Option<&ConnectionProfileId>,
        _search: Option<&str>,
        _status: Option<QueryHistoryStatus>,
    ) -> Result<u64, gluon::AppError> {
        Ok(0)
    }

    async fn find(
        &self,
        _owner: Option<&str>,
        _profile_id: &ConnectionProfileId,
        _id: &QueryHistoryId,
    ) -> Result<Option<QueryHistory>, gluon::AppError> {
        Ok(None)
    }

    async fn save(&self, _entity: &QueryHistory) -> Result<(), gluon::AppError> {
        Ok(())
    }

    async fn delete(
        &self,
        _owner: Option<&str>,
        _profile_id: Option<&ConnectionProfileId>,
        _id: &QueryHistoryId,
    ) -> Result<(), gluon::AppError> {
        Ok(())
    }

    async fn prune(
        &self,
        _owner: Option<&str>,
        _profile_id: &ConnectionProfileId,
        _keep_newest: u32,
    ) -> Result<(), gluon::AppError> {
        Ok(())
    }
}
