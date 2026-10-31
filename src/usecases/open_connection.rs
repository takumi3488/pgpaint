use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::connection_profile::{
    ConnectionProfileAuth, ConnectionProfileId, ConnectionProfileRepository, SecretCipher,
};
use crate::domain::database::{
    ConnectionKey, DatabaseGateway, DbError, OwnerId, Secret, ServerInfo,
};

pub enum OpenConnectionError {
    Database(DbError),
    Application(gluon::AppError),
}

#[async_trait]
pub trait OpenConnection: Send + Sync {
    /// Opens an owner's profile, using a supplied password or its saved secret.
    ///
    /// # Errors
    /// Returns database connection failures separately from application/profile errors.
    async fn execute(
        &self,
        owner: OwnerId,
        profile_id: ConnectionProfileId,
        password: Option<String>,
    ) -> Result<ServerInfo, OpenConnectionError>;
}

pub struct OpenConnectionImpl {
    profiles: Arc<dyn ConnectionProfileRepository>,
    cipher: Arc<dyn SecretCipher>,
    gateway: Arc<dyn DatabaseGateway>,
}

impl OpenConnectionImpl {
    #[must_use]
    pub fn new(
        profiles: Arc<dyn ConnectionProfileRepository>,
        cipher: Arc<dyn SecretCipher>,
        gateway: Arc<dyn DatabaseGateway>,
    ) -> Self {
        Self {
            profiles,
            cipher,
            gateway,
        }
    }
}

#[async_trait]
impl OpenConnection for OpenConnectionImpl {
    async fn execute(
        &self,
        owner: OwnerId,
        profile_id: ConnectionProfileId,
        password: Option<String>,
    ) -> Result<ServerInfo, OpenConnectionError> {
        let profile = self
            .profiles
            .find(Some(&owner.0), &profile_id)
            .await
            .map_err(OpenConnectionError::Application)?
            .ok_or(OpenConnectionError::Application(gluon::AppError::NotFound))?;

        let secret = match (&profile.auth, password) {
            (ConnectionProfileAuth::Password { .. }, Some(password)) => Some(Secret::new(password)),
            (
                ConnectionProfileAuth::Password {
                    encrypted_secret: Some(encrypted),
                },
                None,
            ) => Some(self.cipher.decrypt(encrypted).map_err(|error| {
                OpenConnectionError::Application(gluon::AppError::Internal(Box::new(error)))
            })?),
            (ConnectionProfileAuth::Password { .. }, None)
            | (ConnectionProfileAuth::DsqlIam { .. }, _) => None,
        };

        let target = profile
            .connect_target(secret)
            .map_err(OpenConnectionError::Application)?;
        let key = ConnectionKey {
            owner,
            profile: profile.id,
        };
        self.gateway
            .connect(&key, target)
            .await
            .map_err(OpenConnectionError::Database)
    }
}
