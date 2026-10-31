use std::{error::Error, future::Future, io, sync::Arc};

use gluon::{ContainerBuilder, testing::TestClient};
use pgpaint::domain::{
    connection_profile::{
        ConnectionProfileRepository, EncryptedSecret, SecretCipher, SecretCipherError,
    },
    database::{DatabaseGateway, OwnerId, Secret},
};
use pgpaint::usecases::open_connection::{OpenConnection, OpenConnectionImpl};
use pgpaint_testkit::{Database, metadata_postgres, postgres};
use sqlx::{PgPool, postgres::PgPoolOptions};

pub mod authentication;

pub mod connections;
pub mod er_diagram;
pub mod explain_views;
pub mod history_saved;
pub mod query_editor;
pub mod schema;
pub mod workspace;

pub type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
pub const LOCAL_OWNER: &str = "local";

pub struct TestApp {
    pub client: TestClient,
    pub _metadata: Database,
    pub engine: Database,
    pub metadata_pool: Arc<PgPool>,
    pub profiles: Arc<dyn ConnectionProfileRepository>,
}

pub fn new_test_app() -> impl Future<Output = TestResult<TestApp>> + Send {
    Box::pin(async {
        let (metadata, engine) = tokio::try_join!(metadata_postgres(), postgres())?;
        let metadata_pool = Arc::new(
            PgPoolOptions::new()
                .connect(&metadata.connection.database_url())
                .await?,
        );
        super::MIGRATOR.run(metadata_pool.as_ref()).await?;

        let mut container = super::wiring::build_container(
            ContainerBuilder::new().bind_instance::<PgPool>(metadata_pool.clone()),
        )
        .build();
        let profiles = container.resolve::<dyn ConnectionProfileRepository>();
        let gateway = container.resolve::<dyn DatabaseGateway>();
        container.override_with::<dyn OpenConnection>(Arc::new(OpenConnectionImpl::new(
            Arc::clone(&profiles),
            Arc::new(FakeSecretCipher),
            Arc::clone(&gateway),
        )));

        let router = super::generated_app::__gluon_router()
            .layer(axum::middleware::from_fn(
                gluon::middleware::csrf::csrf_middleware,
            ))
            .layer(
                tower_sessions::SessionManagerLayer::new(tower_sessions::MemoryStore::default())
                    .with_name("gluon_sid"),
            );
        let client = TestClient::new(router, container)?;

        Ok(TestApp {
            client,
            _metadata: metadata,
            engine,
            metadata_pool,
            profiles,
        })
    })
}

#[must_use]
pub fn local_owner() -> OwnerId {
    OwnerId(LOCAL_OWNER.to_owned())
}

pub fn csrf_token(html: &str) -> Result<String, io::Error> {
    let marker = "name=\"csrf-token\" content=\"";
    let value = html
        .split_once(marker)
        .map(|(_, rest)| rest)
        .and_then(|rest| rest.split_once('\"').map(|(token, _)| token))
        .ok_or_else(|| io::Error::other("CSRF token meta tag missing"))?;
    Ok(value.to_owned())
}

pub struct FakeSecretCipher;

impl SecretCipher for FakeSecretCipher {
    fn encrypt(&self, secret: &Secret) -> Result<EncryptedSecret, SecretCipherError> {
        Ok(EncryptedSecret::new(format!("test:{}", secret.expose())))
    }

    fn decrypt(&self, encrypted: &EncryptedSecret) -> Result<Secret, SecretCipherError> {
        let secret = encrypted
            .expose()
            .strip_prefix("test:")
            .ok_or(SecretCipherError::InvalidEncoding)?;
        Ok(Secret::new(secret.to_owned()))
    }
}
