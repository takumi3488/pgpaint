use std::error::Error;
use std::sync::Arc;

use pgpaint::domain::connection_profile::{
    ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId, ConnectionProfileRepository,
};
use pgpaint::domain::database::{Engine, SslMode};
use pgpaint::domain::query_history::{
    QueryHistory, QueryHistoryId, QueryHistoryRepository, QueryHistoryStatus,
};
use pgpaint::domain::saved_query::{SavedQuery, SavedQueryId, SavedQueryRepository};
use pgpaint::infrastructure::persistence::{
    connection_profile_repository::PostgresConnectionProfileRepository,
    query_history_repository::PostgresQueryHistoryRepository,
    saved_query_repository::PostgresSavedQueryRepository,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

const fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn at(seconds: i64) -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + Duration::seconds(seconds)
}

fn profile(owner: Option<&str>, profile_id: Uuid, name: &str) -> ConnectionProfile {
    let now = at(1);
    ConnectionProfile {
        id: ConnectionProfileId(profile_id),
        owner: owner.map(str::to_owned),
        name: name.to_owned(),
        engine: Engine::Postgres,
        host: "localhost".to_owned(),
        port: 5432,
        database: "app".to_owned(),
        username: "app".to_owned(),
        auth: ConnectionProfileAuth::Password {
            encrypted_secret: None,
        },
        ssl_mode: SslMode::Prefer,
        ssl_root_cert_pem: None,
        read_only: false,
        statement_timeout_ms: Some(5_000),
        color: Some("blue".to_owned()),
        created_at: now,
        updated_at: now,
    }
}

fn history(
    history_id: u128,
    owner: Option<&str>,
    profile_id: Uuid,
    sql: &str,
    executed_at: i64,
    status: QueryHistoryStatus,
) -> QueryHistory {
    QueryHistory {
        id: QueryHistoryId(id(history_id)),
        owner: owner.map(str::to_owned),
        profile_id: ConnectionProfileId(profile_id),
        sql: sql.to_owned(),
        executed_at: at(executed_at),
        elapsed_ms: 17,
        rows: Some(3),
        rows_affected: Some(1),
        status,
        error_message: if status == QueryHistoryStatus::Failed {
            Some("query failed".to_owned())
        } else {
            None
        },
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn metadata_repositories_are_scoped_and_pageable() -> Result<(), Box<dyn Error + Send + Sync>>
{
    let database = pgpaint_testkit::metadata_postgres()
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let pool = Arc::new(
        PgPoolOptions::new()
            .max_connections(5)
            .connect(&database.connection.database_url())
            .await?,
    );
    MIGRATOR.run(pool.as_ref()).await?;

    check_profiles(Arc::clone(&pool)).await?;
    check_saved_queries(Arc::clone(&pool)).await?;
    check_history(Arc::clone(&pool)).await?;
    pool.close().await;
    Ok(())
}

async fn check_profiles(pool: Arc<PgPool>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let profile_id = id(100);
    let repository = PostgresConnectionProfileRepository::new(pool);
    let original = profile(Some("alice"), profile_id, "Original");
    assert!(repository.save(&original).await.is_ok());
    let mut updated = original.clone();
    "Updated".clone_into(&mut updated.name);
    assert!(repository.save(&updated).await.is_ok());

    let read_back = repository
        .find(Some("alice"), &original.id)
        .await?
        .ok_or_else(|| std::io::Error::other("profile upsert did not persist"))?;
    assert_eq!(read_back.name, "Updated");
    assert!(repository.find(Some("bob"), &original.id).await?.is_none());
    assert_eq!(repository.list(Some("alice"), 10).await?.len(), 1);
    assert!(
        repository
            .save(&profile(Some("bob"), profile_id, "Different owner"))
            .await
            .is_err()
    );
    Ok(())
}

async fn check_saved_queries(pool: Arc<PgPool>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let repository = PostgresSavedQueryRepository::new(pool);
    let saved_id = SavedQueryId(id(200));
    let profile_id = ConnectionProfileId(id(100));
    let mut saved = SavedQuery {
        id: saved_id.clone(),
        owner: Some("alice".to_owned()),
        profile_id: Some(profile_id.clone()),
        name: "Example".to_owned(),
        sql: "SELECT 1".to_owned(),
        created_at: at(1),
        updated_at: at(1),
    };
    assert!(repository.save(&saved).await.is_ok());
    saved.created_at = at(9);
    "SELECT 2".clone_into(&mut saved.sql);
    saved.updated_at = at(2);
    assert!(repository.save(&saved).await.is_ok());
    let read_back = repository
        .find(Some("alice"), &saved_id)
        .await?
        .ok_or_else(|| std::io::Error::other("saved query upsert did not persist"))?;
    assert_eq!(read_back.sql, "SELECT 2");
    assert_eq!(read_back.created_at, at(1));
    assert!(repository.find(Some("bob"), &saved_id).await?.is_none());
    assert_eq!(
        repository
            .list(Some("alice"), Some(&profile_id), 10)
            .await?
            .len(),
        1
    );
    assert_eq!(
        repository
            .list_search(Some("alice"), Some(&profile_id), 10, Some("select 2"))
            .await?
            .len(),
        1
    );
    assert!(repository.delete(Some("bob"), &saved_id).await.is_ok());
    assert!(repository.find(Some("alice"), &saved_id).await?.is_some());
    assert!(repository.delete(Some("alice"), &saved_id).await.is_ok());
    assert!(repository.find(Some("alice"), &saved_id).await?.is_none());
    Ok(())
}

async fn check_history(pool: Arc<PgPool>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let repository = PostgresQueryHistoryRepository::new(pool);
    let profile_id = ConnectionProfileId(id(100));
    seed_history(&repository).await?;
    check_history_queries(&repository, &profile_id).await?;
    check_history_deletes(&repository, &profile_id).await?;
    Ok(())
}

async fn seed_history(
    repository: &PostgresQueryHistoryRepository,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for item in [
        history(
            301,
            Some("alice"),
            id(100),
            "SELECT beta",
            20,
            QueryHistoryStatus::Failed,
        ),
        history(
            302,
            Some("alice"),
            id(100),
            "SELECT alpha",
            30,
            QueryHistoryStatus::Succeeded,
        ),
        history(
            303,
            Some("alice"),
            id(100),
            "SELECT alpha latest",
            40,
            QueryHistoryStatus::Canceled,
        ),
        history(
            304,
            Some("bob"),
            id(100),
            "SELECT alpha private",
            50,
            QueryHistoryStatus::Succeeded,
        ),
        history(
            305,
            Some("alice"),
            id(101),
            "SELECT other profile",
            60,
            QueryHistoryStatus::Succeeded,
        ),
    ] {
        repository.save(&item).await?;
    }
    Ok(())
}

async fn check_history_queries(
    repository: &PostgresQueryHistoryRepository,
    profile_id: &ConnectionProfileId,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    check_history_search_and_paging(repository, profile_id).await?;
    check_history_scope_filters(repository, profile_id).await?;
    check_history_pruning(repository, profile_id).await
}

async fn check_history_search_and_paging(
    repository: &PostgresQueryHistoryRepository,
    profile_id: &ConnectionProfileId,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let search_results = repository
        .list(Some("alice"), profile_id, 0, 10, Some("ALPHA"))
        .await?;
    assert_eq!(
        search_results
            .iter()
            .map(|item| item.id.0)
            .collect::<Vec<_>>(),
        vec![id(303), id(302)]
    );
    assert_eq!(search_results[0].status, QueryHistoryStatus::Canceled);
    assert_eq!(search_results[1].rows, Some(3));
    assert_eq!(search_results[1].rows_affected, Some(1));

    let first_page = repository
        .list(Some("alice"), profile_id, 0, 2, None)
        .await?;
    let second_page = repository
        .list(Some("alice"), profile_id, 2, 2, None)
        .await?;
    assert_eq!(first_page[0].id.0, id(303));
    assert_eq!(first_page[1].id.0, id(302));
    assert_eq!(second_page[0].id.0, id(301));
    assert_eq!(
        second_page[0].error_message.as_deref(),
        Some("query failed")
    );
    Ok(())
}

async fn check_history_scope_filters(
    repository: &PostgresQueryHistoryRepository,
    profile_id: &ConnectionProfileId,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    assert_eq!(repository.count(Some("alice"), None, None, None).await?, 4);
    assert_eq!(
        repository
            .count(
                Some("alice"),
                Some(profile_id),
                None,
                Some(QueryHistoryStatus::Succeeded),
            )
            .await?,
        1
    );
    let all_profiles = repository
        .list_filtered(Some("alice"), None, 0, 10, None, None)
        .await?;
    assert_eq!(
        all_profiles
            .iter()
            .map(|item| item.id.0)
            .collect::<Vec<_>>(),
        vec![id(305), id(303), id(302), id(301)]
    );
    let succeeded = repository
        .list_filtered(
            Some("alice"),
            Some(profile_id),
            0,
            10,
            None,
            Some(QueryHistoryStatus::Succeeded),
        )
        .await?;
    assert_eq!(succeeded.len(), 1);
    assert_eq!(succeeded[0].id.0, id(302));
    assert!(
        repository
            .find(Some("alice"), profile_id, &QueryHistoryId(id(301)))
            .await?
            .is_some()
    );
    assert!(
        repository
            .find(Some("bob"), profile_id, &QueryHistoryId(id(301)))
            .await?
            .is_none()
    );
    Ok(())
}

async fn check_history_pruning(
    repository: &PostgresQueryHistoryRepository,
    profile_id: &ConnectionProfileId,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    repository.prune(Some("alice"), profile_id, 2).await?;
    let retained = repository
        .list(Some("alice"), profile_id, 0, 10, None)
        .await?;
    assert_eq!(retained.len(), 2);
    assert_eq!(retained[0].id.0, id(303));
    assert_eq!(retained[1].id.0, id(302));
    assert_eq!(
        repository
            .list(Some("bob"), profile_id, 0, 10, None)
            .await?
            .len(),
        1
    );
    assert_eq!(
        repository
            .list(Some("alice"), &ConnectionProfileId(id(101)), 0, 10, None,)
            .await?
            .len(),
        1
    );
    Ok(())
}

async fn check_history_deletes(
    repository: &PostgresQueryHistoryRepository,
    profile_id: &ConnectionProfileId,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    repository
        .delete(Some("bob"), None, &QueryHistoryId(id(303)))
        .await?;
    repository
        .delete(Some("alice"), Some(profile_id), &QueryHistoryId(id(303)))
        .await?;
    assert!(
        repository
            .find(Some("alice"), profile_id, &QueryHistoryId(id(303)))
            .await?
            .is_none()
    );

    let other_profile_id = ConnectionProfileId(id(101));
    repository
        .delete(Some("alice"), Some(profile_id), &QueryHistoryId(id(305)))
        .await?;
    assert!(
        repository
            .find(Some("alice"), &other_profile_id, &QueryHistoryId(id(305)),)
            .await?
            .is_some()
    );
    repository
        .delete(
            Some("alice"),
            Some(&other_profile_id),
            &QueryHistoryId(id(305)),
        )
        .await?;
    assert!(
        repository
            .find(Some("alice"), &other_profile_id, &QueryHistoryId(id(305)),)
            .await?
            .is_none()
    );
    Ok(())
}
