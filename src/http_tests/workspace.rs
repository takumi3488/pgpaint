use axum::http::StatusCode;
use pgpaint::domain::{
    connection_profile::{
        ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId, EncryptedSecret,
    },
    database::{Engine, SslMode},
};
use pgpaint_testkit::Authentication;
use uuid::Uuid;

use super::{TestResult, csrf_token, local_owner, new_test_app};

#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::too_many_lines)]
async fn connect_form_connect_disconnect_and_saved_password_flow() -> TestResult<()> {
    let fixture = new_test_app().await?;
    let Authentication::Password { password } = &fixture.engine.connection.auth else {
        return Err(
            std::io::Error::other("PostgreSQL testkit requires password authentication").into(),
        );
    };
    let profile_id = ConnectionProfileId(Uuid::new_v4());
    let profile = ConnectionProfile::new(
        profile_id.clone(),
        Some(local_owner().0),
        "HTTP test".to_owned(),
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
    let saved_profile_id = ConnectionProfileId(Uuid::new_v4());
    let saved_profile = ConnectionProfile::new(
        saved_profile_id.clone(),
        Some(local_owner().0),
        "Saved password HTTP test".to_owned(),
        Engine::Postgres,
        fixture.engine.connection.host.clone(),
        u32::from(fixture.engine.connection.port),
        fixture.engine.connection.database.clone(),
        fixture.engine.connection.user.clone(),
        ConnectionProfileAuth::Password {
            encrypted_secret: Some(EncryptedSecret::new("test:postgres".to_owned())),
        },
        SslMode::Disable,
        None,
        false,
        None,
        None,
    )?;
    fixture.profiles.save(&profile).await?;
    fixture.profiles.save(&saved_profile).await?;

    let workspace_path = format!("/c/{}", profile_id.0);
    let form = fixture.client.server().get(&workspace_path).await;
    assert_eq!(form.status_code(), StatusCode::OK, "{}", form.text());
    assert!(form.text().contains("name=\"password\""), "{}", form.text());
    assert!(form.text().contains("name=\"_csrf\""), "{}", form.text());
    let csrf_token = csrf_token(&form.text())?;
    let session_cookie = form
        .cookies()
        .get("gluon_sid")
        .map(|cookie| cookie.value().to_owned())
        .ok_or_else(|| std::io::Error::other("session cookie missing"))?;
    let mut cookie_header = format!("gluon_sid={session_cookie}");

    let connected = fixture
        .client
        .server()
        .post(&workspace_path)
        .add_header("cookie", cookie_header.clone())
        .add_header("x-csrf-token", csrf_token.clone())
        .form(&[("password", password.as_str())])
        .await;
    assert_eq!(
        connected.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        connected.text()
    );
    if let Some(cookie) = connected.cookies().get("gluon_sid") {
        cookie_header = format!("gluon_sid={}", cookie.value());
    }

    let active = fixture
        .client
        .server()
        .get(&workspace_path)
        .add_header("cookie", cookie_header.clone())
        .await;
    assert_eq!(
        active.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        active.text()
    );
    if let Some(cookie) = active.cookies().get("gluon_sid") {
        cookie_header = format!("gluon_sid={}", cookie.value());
    }

    let disconnected = fixture
        .client
        .server()
        .post(&format!("{workspace_path}/disconnect"))
        .add_header("cookie", cookie_header.clone())
        .add_header("x-csrf-token", csrf_token.clone())
        .await;
    assert_eq!(
        disconnected.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        disconnected.text()
    );
    if let Some(cookie) = disconnected.cookies().get("gluon_sid") {
        cookie_header = format!("gluon_sid={}", cookie.value());
    }
    let reconnect_form = fixture
        .client
        .server()
        .get(&workspace_path)
        .add_header("cookie", cookie_header.clone())
        .await;
    assert_eq!(
        reconnect_form.status_code(),
        StatusCode::OK,
        "{}",
        reconnect_form.text()
    );
    assert!(
        reconnect_form.text().contains("name=\"password\""),
        "{}",
        reconnect_form.text()
    );

    let saved_workspace_path = format!("/c/{}", saved_profile_id.0);
    let auto_connected = fixture
        .client
        .server()
        .get(&saved_workspace_path)
        .add_header("cookie", cookie_header.clone())
        .await;
    assert_eq!(
        auto_connected.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        auto_connected.text()
    );
    if let Some(cookie) = auto_connected.cookies().get("gluon_sid") {
        cookie_header = format!("gluon_sid={}", cookie.value());
    }
    let saved_disconnected = fixture
        .client
        .server()
        .post(&format!("{saved_workspace_path}/disconnect"))
        .add_header("cookie", cookie_header.clone())
        .add_header("x-csrf-token", csrf_token.clone())
        .await;
    assert_eq!(
        saved_disconnected.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        saved_disconnected.text()
    );
    if let Some(cookie) = saved_disconnected.cookies().get("gluon_sid") {
        cookie_header = format!("gluon_sid={}", cookie.value());
    }
    let saved_connect_form = fixture
        .client
        .server()
        .get(&saved_workspace_path)
        .add_header("cookie", cookie_header.clone())
        .await;
    assert_eq!(
        saved_connect_form.status_code(),
        StatusCode::OK,
        "{}",
        saved_connect_form.text()
    );
    assert!(
        !saved_connect_form.text().contains("name=\"password\""),
        "{}",
        saved_connect_form.text()
    );
    let saved_connected = fixture
        .client
        .server()
        .post(&saved_workspace_path)
        .add_header("cookie", cookie_header.clone())
        .add_header("x-csrf-token", csrf_token.clone())
        .form(&[("_csrf", csrf_token.as_str())])
        .await;
    assert_eq!(
        saved_connected.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        saved_connected.text()
    );
    let saved_active = fixture
        .client
        .server()
        .get(&saved_workspace_path)
        .add_header("cookie", cookie_header)
        .await;
    assert_eq!(
        saved_active.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        saved_active.text()
    );
    Ok(())
}
