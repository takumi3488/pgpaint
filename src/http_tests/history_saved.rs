use axum::http::StatusCode;
use pgpaint::domain::{
    connection_profile::{ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId},
    database::{Engine, SslMode},
    query_history::{QueryHistory, QueryHistoryId, QueryHistoryRepository, QueryHistoryStatus},
    saved_query::{SavedQuery, SavedQueryId, SavedQueryRepository},
};
use pgpaint::infrastructure::persistence::{
    query_history_repository::PostgresQueryHistoryRepository,
    saved_query_repository::PostgresSavedQueryRepository,
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use super::{LOCAL_OWNER, TestResult, csrf_token, new_test_app};

#[tokio::test(flavor = "multi_thread")]
async fn history_and_saved_routes_scope_owners_and_page_filtered_history() -> TestResult<()> {
    let fixture = new_test_app().await?;
    let (profile_id, foreign_history_id) = seed_history_fixture(&fixture).await?;
    let (cookie, csrf) =
        exercise_history_routes(&fixture, &profile_id, &foreign_history_id).await?;
    exercise_saved_routes(&fixture, &profile_id, &cookie, &csrf).await?;
    Ok(())
}

async fn seed_history_fixture(
    fixture: &super::TestApp,
) -> TestResult<(ConnectionProfileId, QueryHistoryId)> {
    let profile_id = ConnectionProfileId(Uuid::new_v4());
    let profile = ConnectionProfile::new(
        profile_id.clone(),
        Some(LOCAL_OWNER.to_owned()),
        "History HTTP test".to_owned(),
        Engine::Postgres,
        fixture.engine.connection.host.clone(),
        u32::from(fixture.engine.connection.port),
        fixture.engine.connection.database.clone(),
        fixture.engine.connection.user.clone(),
        ConnectionProfileAuth::Password {
            encrypted_secret: None,
        },
        SslMode::Disable,
        None,
        false,
        None,
        None,
    )?;
    fixture.profiles.save(&profile).await?;

    let history_repository = PostgresQueryHistoryRepository::new(fixture.metadata_pool.clone());
    for seconds in 0_i64..21 {
        history_repository
            .save(&history(
                Uuid::new_v4(),
                LOCAL_OWNER,
                &profile_id,
                format!("SELECT local {seconds}"),
                seconds,
            ))
            .await?;
    }
    let foreign_history_id = QueryHistoryId(Uuid::new_v4());
    history_repository
        .save(&history(
            foreign_history_id.0,
            "other-owner",
            &profile_id,
            "SELECT other owner".to_owned(),
            100,
        ))
        .await?;
    Ok((profile_id, foreign_history_id))
}

async fn exercise_history_routes(
    fixture: &super::TestApp,
    profile_id: &ConnectionProfileId,
    foreign_history_id: &QueryHistoryId,
) -> TestResult<(String, String)> {
    let history_repository = PostgresQueryHistoryRepository::new(fixture.metadata_pool.clone());
    let history_path = format!(
        "/history?profile_id={}&status=failed&page=999",
        profile_id.0
    );
    let history_page = fixture.client.server().get(&history_path).await;
    assert_eq!(
        history_page.status_code(),
        StatusCode::OK,
        "{}",
        history_page.text()
    );
    let body = history_page.text();
    assert!(body.contains("Page 2 of 2"), "{body}");
    assert!(body.contains("SELECT local 0"), "{body}");
    assert!(!body.contains("SELECT local 20"), "{body}");
    assert!(!body.contains("SELECT other owner"), "{body}");
    let csrf = csrf_token(&body)?;
    let session_cookie = history_page
        .cookies()
        .get("gluon_sid")
        .map(|cookie| cookie.value().to_owned())
        .ok_or_else(|| std::io::Error::other("session cookie missing"))?;
    let cookie = format!("gluon_sid={session_cookie}");
    let foreign_history_id_text = foreign_history_id.0.to_string();
    let profile_id_text = profile_id.0.to_string();

    let delete_foreign = fixture
        .client
        .server()
        .post("/history")
        .add_header("cookie", cookie.clone())
        .add_header("x-csrf-token", csrf.clone())
        .add_header("hx-request", "true")
        .form(&[
            ("_csrf", csrf.as_str()),
            ("operation", "delete"),
            ("history_id", foreign_history_id_text.as_str()),
            ("profile_id", profile_id_text.as_str()),
            ("search", ""),
            ("status", "failed"),
            ("page", "1"),
        ])
        .await;
    assert_eq!(delete_foreign.status_code(), StatusCode::OK);
    assert!(
        history_repository
            .find(Some("other-owner"), profile_id, foreign_history_id)
            .await?
            .is_some()
    );

    let clear_profile = fixture
        .client
        .server()
        .post("/history")
        .add_header("cookie", cookie.clone())
        .add_header("x-csrf-token", csrf.clone())
        .form(&[
            ("_csrf", csrf.as_str()),
            ("operation", "clear-profile"),
            ("profile_id", profile_id_text.as_str()),
            ("search", ""),
            ("status", ""),
            ("page", "1"),
        ])
        .await;
    assert_eq!(clear_profile.status_code(), StatusCode::SEE_OTHER);
    assert_eq!(
        history_repository
            .count(Some(LOCAL_OWNER), Some(profile_id), None, None)
            .await?,
        0
    );
    assert_eq!(
        history_repository
            .count(Some("other-owner"), Some(profile_id), None, None)
            .await?,
        1
    );
    Ok((cookie, csrf))
}

async fn exercise_saved_routes(
    fixture: &super::TestApp,
    profile_id: &ConnectionProfileId,
    cookie: &str,
    csrf: &str,
) -> TestResult<()> {
    let saved_id = SavedQueryId(Uuid::new_v4());
    let now = OffsetDateTime::now_utc();
    let saved_repository = PostgresSavedQueryRepository::new(fixture.metadata_pool.clone());
    saved_repository
        .save(&SavedQuery {
            id: saved_id.clone(),
            owner: Some("other-owner".to_owned()),
            profile_id: Some(profile_id.clone()),
            name: "Private query".to_owned(),
            sql: "SELECT private".to_owned(),
            created_at: now,
            updated_at: now,
        })
        .await?;
    let private_query = fixture
        .client
        .server()
        .get(&format!("/saved/{}", saved_id.0))
        .add_header("cookie", cookie.to_owned())
        .await;
    assert_eq!(private_query.status_code(), StatusCode::NOT_FOUND);

    let invalid_saved = fixture
        .client
        .server()
        .post("/saved")
        .add_header("cookie", cookie.to_owned())
        .add_header("x-csrf-token", csrf.to_owned())
        .form(&[
            ("_csrf", csrf),
            ("operation", "create"),
            ("name", "  "),
            ("sql", "SELECT 1"),
            ("profile_id", ""),
        ])
        .await;
    assert_eq!(
        invalid_saved.status_code(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    Ok(())
}

fn history(
    id: Uuid,
    owner: &str,
    profile_id: &ConnectionProfileId,
    sql: String,
    executed_at_seconds: i64,
) -> QueryHistory {
    QueryHistory {
        id: QueryHistoryId(id),
        owner: Some(owner.to_owned()),
        profile_id: profile_id.clone(),
        sql,
        executed_at: OffsetDateTime::UNIX_EPOCH + Duration::seconds(executed_at_seconds),
        elapsed_ms: 1,
        rows: Some(1),
        rows_affected: None,
        status: QueryHistoryStatus::Failed,
        error_message: Some("expected test failure".to_owned()),
    }
}
