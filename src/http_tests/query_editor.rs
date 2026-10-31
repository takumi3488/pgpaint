use std::time::Duration;

use axum::http::StatusCode;
use pgpaint::domain::{
    connection_profile::{
        ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId, EncryptedSecret,
    },
    database::{Engine, SslMode},
    query_history::{QueryHistory, QueryHistoryId, QueryHistoryRepository, QueryHistoryStatus},
    saved_query::{SavedQuery, SavedQueryId, SavedQueryRepository},
};
use pgpaint::infrastructure::persistence::{
    query_history_repository::PostgresQueryHistoryRepository,
    saved_query_repository::PostgresSavedQueryRepository,
};
use pgpaint_testkit::Authentication;
use time::OffsetDateTime;
use uuid::Uuid;

use super::{LOCAL_OWNER, TestResult, csrf_token, new_test_app};

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread")]
async fn query_editor_loads_owner_scoped_sql_and_pages_run_results() -> TestResult<()> {
    let fixture = new_test_app().await?;
    let Authentication::Password { password } = &fixture.engine.connection.auth else {
        return Err(
            std::io::Error::other("PostgreSQL testkit requires password authentication").into(),
        );
    };
    let profile_id = ConnectionProfileId(Uuid::new_v4());
    let profile = ConnectionProfile::new(
        profile_id.clone(),
        Some(LOCAL_OWNER.to_owned()),
        "Query editor HTTP test".to_owned(),
        Engine::Postgres,
        fixture.engine.connection.host.clone(),
        u32::from(fixture.engine.connection.port),
        fixture.engine.connection.database.clone(),
        fixture.engine.connection.user.clone(),
        ConnectionProfileAuth::Password {
            encrypted_secret: Some(EncryptedSecret::new(format!("test:{password}"))),
        },
        SslMode::Disable,
        None,
        false,
        None,
        None,
    )?;
    fixture.profiles.save(&profile).await?;

    let history = PostgresQueryHistoryRepository::new(fixture.metadata_pool.clone());
    let saved = PostgresSavedQueryRepository::new(fixture.metadata_pool.clone());
    let now = OffsetDateTime::now_utc();
    let history_id = QueryHistoryId(Uuid::new_v4());
    history
        .save(&QueryHistory {
            id: history_id.clone(),
            owner: Some(LOCAL_OWNER.to_owned()),
            profile_id: profile_id.clone(),
            sql: "SELECT 73".to_owned(),
            executed_at: now,
            elapsed_ms: 2,
            rows: Some(1),
            rows_affected: None,
            status: QueryHistoryStatus::Succeeded,
            error_message: None,
        })
        .await?;
    let foreign_history_id = QueryHistoryId(Uuid::new_v4());
    history
        .save(&QueryHistory {
            id: foreign_history_id.clone(),
            owner: Some("other-owner".to_owned()),
            profile_id: profile_id.clone(),
            sql: "SELECT 74".to_owned(),
            executed_at: now,
            elapsed_ms: 2,
            rows: Some(1),
            rows_affected: None,
            status: QueryHistoryStatus::Succeeded,
            error_message: None,
        })
        .await?;
    let saved_id = SavedQueryId(Uuid::new_v4());
    saved
        .save(&SavedQuery {
            id: saved_id.clone(),
            owner: Some(LOCAL_OWNER.to_owned()),
            profile_id: Some(profile_id.clone()),
            name: "Saved editor SQL".to_owned(),
            sql: "SELECT 42".to_owned(),
            created_at: now,
            updated_at: now,
        })
        .await?;
    let foreign_saved_id = SavedQueryId(Uuid::new_v4());
    saved
        .save(&SavedQuery {
            id: foreign_saved_id.clone(),
            owner: Some("other-owner".to_owned()),
            profile_id: Some(profile_id.clone()),
            name: "Foreign editor SQL".to_owned(),
            sql: "SELECT 43".to_owned(),
            created_at: now,
            updated_at: now,
        })
        .await?;

    let query_path = format!("/c/{}/query", profile_id.0);
    let history_page = fixture
        .client
        .server()
        .get(&format!("{query_path}?history={}", history_id.0))
        .await;
    assert_eq!(
        history_page.status_code(),
        StatusCode::OK,
        "{}",
        history_page.text()
    );
    let history_html = history_page.text();
    assert!(history_html.contains("SELECT 73"), "{history_html}");
    assert!(!history_html.contains("SELECT 74"), "{history_html}");
    let csrf = csrf_token(&history_html)?;
    let session_cookie = history_page
        .cookies()
        .get("gluon_sid")
        .map(|cookie| cookie.value().to_owned())
        .ok_or_else(|| std::io::Error::other("session cookie missing"))?;
    let cookie = format!("gluon_sid={session_cookie}");

    let foreign_history = fixture
        .client
        .server()
        .get(&format!("{query_path}?history={}", foreign_history_id.0))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(
        foreign_history.status_code(),
        StatusCode::NOT_FOUND,
        "{}",
        foreign_history.text()
    );
    let saved_page = fixture
        .client
        .server()
        .get(&format!("{query_path}?saved={}", saved_id.0))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(
        saved_page.status_code(),
        StatusCode::OK,
        "{}",
        saved_page.text()
    );
    assert!(saved_page.text().contains("SELECT 42"));
    let foreign_saved = fixture
        .client
        .server()
        .get(&format!("{query_path}?saved={}", foreign_saved_id.0))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(
        foreign_saved.status_code(),
        StatusCode::NOT_FOUND,
        "{}",
        foreign_saved.text()
    );
    let completion = fixture
        .client
        .server()
        .get(&format!("/c/{}/completion", profile_id.0))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(
        completion.status_code(),
        StatusCode::OK,
        "{}",
        completion.text()
    );
    let completion_json: serde_json::Value = serde_json::from_str(&completion.text())?;
    assert!(completion_json.get("schemas").is_some());

    let save_response = fixture
        .client
        .server()
        .post(&query_path)
        .add_header("cookie", cookie.clone())
        .add_header("x-csrf-token", csrf.clone())
        .form(&[
            ("_csrf", csrf.as_str()),
            ("action", "save"),
            ("name", "Saved from editor"),
            ("sql", "SELECT 44"),
        ])
        .await;
    assert_eq!(save_response.status_code(), StatusCode::SEE_OTHER);
    let saved_by_editor = saved
        .list(Some(LOCAL_OWNER), Some(&profile_id), 100)
        .await?
        .into_iter()
        .find(|query| query.name == "Saved from editor")
        .ok_or_else(|| std::io::Error::other("query save was not persisted"))?;
    let reloaded_saved = fixture
        .client
        .server()
        .get(&format!("{query_path}?saved={}", saved_by_editor.id.0))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(reloaded_saved.status_code(), StatusCode::OK);
    assert!(reloaded_saved.text().contains("SELECT 44"));

    let post = fixture
        .client
        .server()
        .post(&query_path)
        .add_header("cookie", cookie.clone())
        .add_header("x-csrf-token", csrf.clone())
        .form(&[
            ("_csrf", csrf.as_str()),
            ("sql", "SELECT generate_series(1, 205) AS n"),
        ])
        .await;
    assert_eq!(post.status_code(), StatusCode::OK, "{}", post.text());
    let response_html = post.text();
    let events_url = response_html
        .split("sse-connect=\"")
        .nth(1)
        .and_then(|value| value.split_once('\"').map(|(url, _)| url))
        .ok_or_else(|| std::io::Error::other("query response did not include an SSE URL"))?;
    let run_url = events_url
        .strip_suffix("/events")
        .ok_or_else(|| std::io::Error::other("unexpected query SSE URL"))?;
    let result_url = format!("{run_url}?statement=0&page=999");
    let result_html = poll_result(&fixture, &cookie, &result_url).await?;
    assert!(result_html.contains("Page 3 of 3"), "{result_html}");
    assert!(result_html.contains("205"), "{result_html}");
    let csv = fixture
        .client
        .server()
        .get(&format!("{run_url}/export?statement=0&format=csv"))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(csv.status_code(), StatusCode::OK, "{}", csv.text());
    let csv_body = csv.text();
    assert!(csv_body.contains("\"n\""));
    assert!(csv_body.contains("205"));

    let json = fixture
        .client
        .server()
        .get(&format!("{run_url}/export?statement=0&format=json"))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(json.status_code(), StatusCode::OK, "{}", json.text());
    let json_body = json.text();
    let exported: serde_json::Value = serde_json::from_str(&json_body)?;
    assert_eq!(exported["rows"].as_array().map_or(0, Vec::len), 205);
    let run_history_id = QueryHistoryId(Uuid::parse_str(
        run_url
            .rsplit('/')
            .next()
            .ok_or_else(|| std::io::Error::other("query run id missing"))?,
    )?);
    let mut recorded_history = None;
    for _ in 0..100 {
        recorded_history = history
            .find(Some(LOCAL_OWNER), &profile_id, &run_history_id)
            .await?;
        if recorded_history.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let recorded_history = recorded_history
        .ok_or_else(|| std::io::Error::other("query history entry was not persisted"))?;
    assert_eq!(recorded_history.status, QueryHistoryStatus::Succeeded);
    assert_eq!(recorded_history.rows, Some(205));

    let read_only_id = ConnectionProfileId(Uuid::new_v4());
    fixture
        .profiles
        .save(&ConnectionProfile::new(
            read_only_id.clone(),
            Some(LOCAL_OWNER.to_owned()),
            "Read-only query editor test".to_owned(),
            Engine::Postgres,
            fixture.engine.connection.host.clone(),
            u32::from(fixture.engine.connection.port),
            fixture.engine.connection.database.clone(),
            fixture.engine.connection.user.clone(),
            ConnectionProfileAuth::Password {
                encrypted_secret: Some(EncryptedSecret::new(format!("test:{password}"))),
            },
            SslMode::Disable,
            None,
            true,
            None,
            None,
        )?)
        .await?;
    let read_only_path = format!("/c/{}/query", read_only_id.0);
    let blocked = fixture
        .client
        .server()
        .post(&read_only_path)
        .add_header("cookie", cookie.clone())
        .add_header("x-csrf-token", csrf.clone())
        .form(&[
            ("_csrf", csrf.as_str()),
            ("sql", "DELETE FROM pgpaint_missing_table"),
            ("force", "true"),
        ])
        .await;
    assert_eq!(blocked.status_code(), StatusCode::OK, "{}", blocked.text());
    assert!(blocked.text().contains("read-only"), "{}", blocked.text());
    assert!(!blocked.text().contains("Run anyway"), "{}", blocked.text());
    Ok(())
}

async fn poll_result(
    fixture: &super::TestApp,
    cookie: &str,
    result_url: &str,
) -> TestResult<String> {
    for _ in 0..100 {
        let response = fixture
            .client
            .server()
            .get(result_url)
            .add_header("cookie", cookie.to_owned())
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "{}",
            response.text()
        );
        let html = response.text();
        if html.contains("Completed") {
            return Ok(html);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(std::io::Error::other("query did not complete during the test timeout").into())
}
