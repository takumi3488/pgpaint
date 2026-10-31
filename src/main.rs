use std::sync::Arc;

use gluon::{AppError, Boot};
// `app/**` handlers resolve `crate::{domain, usecases, ...}` through these re-exports.
#[allow(unused_imports)]
use pgpaint::{auth, domain, dto, infrastructure, usecases, web, wiring};

#[allow(clippy::redundant_static_lifetimes, clippy::too_many_lines)]
mod generated_app {
    gluon::app!();
}

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

#[tokio::main(flavor = "multi_thread")]
async fn main() -> gluon::Result<()> {
    migrate_metadata().await?;
    let (auth_config, auth_ui) = auth::from_env().await.map_err(AppError::Internal)?;
    let auth_ui = Arc::new(auth_ui);
    let boot = Boot::new()
        .with_container(move |builder| wiring::build_container(builder).bind_instance(auth_ui))
        .with_router(generated_app::__gluon_router());
    let boot = if let Some(config) = auth_config {
        boot.with_auth(config)
    } else {
        boot
    };
    boot.run().await
}

async fn migrate_metadata() -> gluon::Result<()> {
    let database_url =
        std::env::var("DATABASE_URL").map_err(|error| AppError::Internal(Box::new(error)))?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&database_url)
        .await
        .map_err(|error| AppError::Internal(Box::new(error)))?;
    MIGRATOR
        .run(&pool)
        .await
        .map_err(|error| AppError::Internal(Box::new(error)))?;
    pool.close().await;
    Ok(())
}

#[cfg(test)]
mod http_tests;
