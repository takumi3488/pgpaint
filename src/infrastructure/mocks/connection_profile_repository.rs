use crate::domain::connection_profile::{
    ConnectionProfile, ConnectionProfileId, ConnectionProfileRepository,
};
use async_trait::async_trait;

#[derive(Default)]
pub struct MockConnectionProfileRepository;

impl MockConnectionProfileRepository {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ConnectionProfileRepository for MockConnectionProfileRepository {
    async fn list(
        &self,
        _owner: Option<&str>,
        _limit: u32,
    ) -> Result<Vec<ConnectionProfile>, gluon::AppError> {
        Ok(vec![])
    }

    async fn find(
        &self,
        _owner: Option<&str>,
        _id: &ConnectionProfileId,
    ) -> Result<Option<ConnectionProfile>, gluon::AppError> {
        Ok(None)
    }

    async fn save(&self, _entity: &ConnectionProfile) -> Result<(), gluon::AppError> {
        Ok(())
    }

    async fn delete(
        &self,
        _owner: Option<&str>,
        _id: &ConnectionProfileId,
    ) -> Result<(), gluon::AppError> {
        Ok(())
    }
}
