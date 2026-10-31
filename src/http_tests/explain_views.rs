use axum::http::StatusCode;
use pgpaint::domain::{
    connection_profile::{
        ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId, EncryptedSecret,
    },
    database::{Engine, SslMode},
};
use pgpaint_testkit::{Authentication, seed};
use uuid::Uuid;

use super::{LOCAL_OWNER, TestResult, csrf_token, new_test_app};

#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::too_many_lines)]
async fn read_only_analyze_is_refused_and_engine_views_render() -> TestResult<()> {
    let fixture = new_test_app().await?;
    seed(&fixture.engine).await?;
    let Authentication::Password { password } = &fixture.engine.connection.auth else {
        return Err(
            std::io::Error::other("PostgreSQL testkit requires password authentication").into(),
        );
    };
    let profile_id = ConnectionProfileId(Uuid::new_v4());
    let profile = ConnectionProfile::new(
        profile_id.clone(),
        Some(LOCAL_OWNER.to_owned()),
        "Explain HTTP test".to_owned(),
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
    )?;
    fixture.profiles.save(&profile).await?;

    let home = fixture.client.server().get("/").await;
    assert_eq!(home.status_code(), StatusCode::OK, "{}", home.text());
    let csrf = csrf_token(&home.text())?;
    let cookie = home
        .cookies()
        .get("gluon_sid")
        .map(|cookie| format!("gluon_sid={}", cookie.value()))
        .ok_or_else(|| std::io::Error::other("session cookie missing"))?;

    let activity = fixture
        .client
        .server()
        .get(&format!("/c/{}/views/activity", profile_id.0))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(
        activity.status_code(),
        StatusCode::OK,
        "{}",
        activity.text()
    );
    assert!(activity.text().contains("Activity"), "{}", activity.text());
    assert!(activity.text().contains("every 5s"), "{}", activity.text());

    let refreshed = fixture
        .client
        .server()
        .get(&format!("/c/{}/views/activity", profile_id.0))
        .add_header("cookie", cookie.clone())
        .add_header("hx-request", "true")
        .await;
    assert_eq!(
        refreshed.status_code(),
        StatusCode::OK,
        "{}",
        refreshed.text()
    );
    assert!(
        refreshed.text().contains("id=\"view-results\""),
        "{}",
        refreshed.text()
    );

    let unknown = fixture
        .client
        .server()
        .get(&format!("/c/{}/views/not_a_view", profile_id.0))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(
        unknown.status_code(),
        StatusCode::NOT_FOUND,
        "{}",
        unknown.text()
    );

    let relation_picker = fixture
        .client
        .server()
        .get(&format!("/c/{}/views/table_stats", profile_id.0))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(
        relation_picker.status_code(),
        StatusCode::OK,
        "{}",
        relation_picker.text()
    );
    assert!(
        relation_picker.text().contains("Choose a relation"),
        "{}",
        relation_picker.text()
    );

    let table_stats = fixture
        .client
        .server()
        .get(&format!(
            "/c/{}/views/table_stats?schema=public&name=customers",
            profile_id.0
        ))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(
        table_stats.status_code(),
        StatusCode::OK,
        "{}",
        table_stats.text()
    );
    assert!(
        table_stats.text().contains("public.customers"),
        "{}",
        table_stats.text()
    );
    let refused = fixture
        .client
        .server()
        .post(&format!("/c/{}/explain", profile_id.0))
        .add_header("cookie", cookie.clone())
        .add_header("x-csrf-token", csrf.clone())
        .add_header("hx-request", "true")
        .form(&[
            ("sql", "UPDATE public.__pgpaint_missing_table SET value = 1"),
            ("analyze", "1"),
            ("confirm", "1"),
        ])
        .await;
    assert_eq!(refused.status_code(), StatusCode::OK, "{}", refused.text());
    assert!(
        refused
            .text()
            .contains("cannot run DML or DDL on a read-only profile"),
        "{}",
        refused.text()
    );

    let batch = fixture
        .client
        .server()
        .post(&format!("/c/{}/explain", profile_id.0))
        .add_header("cookie", cookie)
        .add_header("x-csrf-token", csrf)
        .add_header("hx-request", "true")
        .form(&[("sql", "SELECT 1; SELECT 2")])
        .await;
    assert_eq!(batch.status_code(), StatusCode::OK, "{}", batch.text());
    assert!(
        batch.text().contains("exactly one SQL statement"),
        "{}",
        batch.text()
    );
    Ok(())
}
