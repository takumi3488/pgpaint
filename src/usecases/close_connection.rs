use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::connection_profile::ConnectionProfileId;
use crate::domain::database::{ConnectionKey, DatabaseGateway, OwnerId};

#[async_trait]
pub trait CloseConnection: Send + Sync {
    async fn execute(&self, owner: OwnerId, profile_id: ConnectionProfileId);
}

pub struct CloseConnectionImpl {
    gateway: Arc<dyn DatabaseGateway>,
}

impl CloseConnectionImpl {
    #[must_use]
    pub fn new(gateway: Arc<dyn DatabaseGateway>) -> Self {
        Self { gateway }
    }
}

#[async_trait]
impl CloseConnection for CloseConnectionImpl {
    async fn execute(&self, owner: OwnerId, profile_id: ConnectionProfileId) {
        self.gateway
            .disconnect(&ConnectionKey {
                owner,
                profile: profile_id,
            })
            .await;
    }
}
