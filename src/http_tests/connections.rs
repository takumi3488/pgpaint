use axum::http::StatusCode;
use pgpaint::domain::{
    connection_profile::{
        ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId, EncryptedSecret,
    },
    database::{Engine, SslMode},
};
use pgpaint_testkit::Authentication;
use uuid::Uuid;

use super::{LOCAL_OWNER, TestApp, TestResult, csrf_token, new_test_app};

#[tokio::test(flavor = "multi_thread")]
async fn connection_profiles_are_owner_scoped_and_validation_renders_inline() -> TestResult<()> {
    let app = new_test_app().await?;
    let private_id = ConnectionProfileId(Uuid::new_v4());
    let foreign_profile = ConnectionProfile::new(
        private_id.clone(),
        Some("another-owner".to_owned()),
        "Private profile".to_owned(),
        Engine::Postgres,
        app.engine.connection.host.clone(),
        u32::from(app.engine.connection.port),
        app.engine.connection.database.clone(),
        app.engine.connection.user.clone(),
        ConnectionProfileAuth::Password {
            encrypted_secret: None,
        },
        SslMode::Disable,
        None,
        false,
        None,
        None,
    )?;
    app.profiles.save(&foreign_profile).await?;

    let list = app.client.server().get("/").await;
    assert_eq!(list.status_code(), StatusCode::OK, "{}", list.text());
    assert!(!list.text().contains("Private profile"));

    let foreign_edit = app
        .client
        .server()
        .get(&format!("/connections/{}/edit", private_id.0))
        .await;
    assert_eq!(foreign_edit.status_code(), StatusCode::NOT_FOUND);

    let editor = app.client.server().get("/connections/new").await;
    assert_eq!(editor.status_code(), StatusCode::OK, "{}", editor.text());
    let csrf = csrf_token(&editor.text())?;
    let session_cookie = editor
        .cookies()
        .get("gluon_sid")
        .map(|cookie| cookie.value().to_owned())
        .ok_or_else(|| std::io::Error::other("session cookie missing"))?;
    let cookie_header = format!("gluon_sid={session_cookie}");
    let denied_delete = app
        .client
        .server()
        .post(&format!("/connections/{}/edit", private_id.0))
        .add_header("cookie", cookie_header.clone())
        .add_header("x-csrf-token", csrf.clone())
        .form(&[("_csrf", csrf.as_str()), ("_action", "delete")])
        .await;
    assert_eq!(denied_delete.status_code(), StatusCode::NOT_FOUND);
    assert!(
        app.profiles
            .find(Some("another-owner"), &private_id)
            .await?
            .is_some()
    );
    let invalid = app
        .client
        .server()
        .post("/connections/new")
        .add_header("cookie", cookie_header.clone())
        .add_header("x-csrf-token", csrf.clone())
        .form(&[
            ("_csrf", csrf.as_str()),
            ("name", ""),
            ("engine", "postgres"),
            ("host", ""),
            ("port", "5432"),
            ("database", ""),
            ("username", ""),
            ("password", ""),
            ("ssl_mode", "prefer"),
            ("ssl_root_cert_pem", ""),
            ("statement_timeout_ms", ""),
            ("color", "#0ea5e9"),
        ])
        .await;
    assert_eq!(
        invalid.status_code(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        invalid.text()
    );
    assert!(invalid.text().contains("Name must not be empty."));
    assert!(invalid.text().contains("Host must not be empty."));
    assert!(!invalid.text().contains("Private profile"));
    let cookie_header = invalid.cookies().get("gluon_sid").map_or_else(
        || cookie_header,
        |cookie| format!("gluon_sid={}", cookie.value()),
    );
    let cookie_header = save_new_profile(&app, &csrf, &cookie_header).await?;
    let cookie_header = test_connection_fragment(&app, &csrf, &cookie_header).await?;
    assert_saved_password_update(&app, &csrf, &cookie_header).await?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn assert_saved_password_update(
    app: &TestApp,
    csrf: &str,
    cookie_header: &str,
) -> TestResult<()> {
    let id = ConnectionProfileId(Uuid::new_v4());
    app.profiles
        .save(&saved_password_profile(app, id.clone())?)
        .await?;

    let path = format!("/connections/{}/edit", id.0);
    let editor = app
        .client
        .server()
        .get(&path)
        .add_header("cookie", cookie_header)
        .await;
    assert_eq!(editor.status_code(), StatusCode::OK, "{}", editor.text());
    assert!(editor.text().contains("A password is already saved."));
    assert!(!editor.text().contains("source-secret"));
    let cookie_header = editor.cookies().get("gluon_sid").map_or_else(
        || cookie_header.to_owned(),
        |cookie| format!("gluon_sid={}", cookie.value()),
    );

    let source_id = id.0.to_string();
    let port = app.engine.connection.port.to_string();
    let form = vec![
        ("_csrf", csrf),
        ("source_profile_id", source_id.as_str()),
        ("name", "Updated profile"),
        ("engine", "postgres"),
        ("host", app.engine.connection.host.as_str()),
        ("port", port.as_str()),
        ("database", app.engine.connection.database.as_str()),
        ("username", app.engine.connection.user.as_str()),
        ("password", ""),
        ("ssl_mode", "disable"),
        ("ssl_root_cert_pem", ""),
        ("statement_timeout_ms", ""),
        ("color", "#0ea5e9"),
    ];
    let updated = app
        .client
        .server()
        .post(&path)
        .add_header("cookie", cookie_header.clone())
        .add_header("x-csrf-token", csrf.to_owned())
        .form(&form)
        .await;
    assert_eq!(
        updated.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        updated.text()
    );
    let saved = app
        .profiles
        .find(Some(LOCAL_OWNER), &id)
        .await?
        .ok_or_else(|| std::io::Error::other("updated profile missing"))?;
    let ConnectionProfileAuth::Password {
        encrypted_secret: Some(secret),
    } = saved.auth
    else {
        return Err(std::io::Error::other("saved password was not retained").into());
    };
    assert_eq!(secret.expose(), "test:source-secret");

    let cookie_header = updated.cookies().get("gluon_sid").map_or_else(
        || cookie_header.clone(),
        |cookie| format!("gluon_sid={}", cookie.value()),
    );
    let mut clear_form = form;
    clear_form.push(("clear_saved_password", "on"));
    let cleared = app
        .client
        .server()
        .post(&path)
        .add_header("cookie", cookie_header.clone())
        .add_header("x-csrf-token", csrf.to_owned())
        .form(&clear_form)
        .await;
    assert_eq!(
        cleared.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        cleared.text()
    );
    let cleared_profile = app
        .profiles
        .find(Some(LOCAL_OWNER), &id)
        .await?
        .ok_or_else(|| std::io::Error::other("cleared profile missing"))?;
    let ConnectionProfileAuth::Password {
        encrypted_secret: None,
    } = cleared_profile.auth
    else {
        return Err(std::io::Error::other("saved password was not cleared").into());
    };
    let cookie_header = cleared
        .cookies()
        .get("gluon_sid")
        .map_or(cookie_header, |cookie| {
            format!("gluon_sid={}", cookie.value())
        });
    delete_connection_profile(app, &path, &id, csrf, &cookie_header).await
}

async fn delete_connection_profile(
    app: &TestApp,
    path: &str,
    id: &ConnectionProfileId,
    csrf: &str,
    cookie: &str,
) -> TestResult<()> {
    let response = app
        .client
        .server()
        .post(path)
        .add_header("cookie", cookie.to_owned())
        .add_header("x-csrf-token", csrf.to_owned())
        .form(&[("_csrf", csrf), ("_action", "delete")])
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        response.text()
    );
    assert!(app.profiles.find(Some(LOCAL_OWNER), id).await?.is_none());
    Ok(())
}

fn saved_password_profile(app: &TestApp, id: ConnectionProfileId) -> TestResult<ConnectionProfile> {
    Ok(ConnectionProfile::new(
        id,
        Some(LOCAL_OWNER.to_owned()),
        "Saved profile".to_owned(),
        Engine::Postgres,
        app.engine.connection.host.clone(),
        u32::from(app.engine.connection.port),
        app.engine.connection.database.clone(),
        app.engine.connection.user.clone(),
        ConnectionProfileAuth::Password {
            encrypted_secret: Some(EncryptedSecret::new("test:source-secret".to_owned())),
        },
        SslMode::Disable,
        None,
        false,
        None,
        Some("#0ea5e9".to_owned()),
    )?)
}

async fn save_new_profile(app: &TestApp, csrf: &str, cookie: &str) -> TestResult<String> {
    let port = app.engine.connection.port.to_string();
    let response = app
        .client
        .server()
        .post("/connections/new")
        .add_header("cookie", cookie.to_owned())
        .add_header("x-csrf-token", csrf.to_owned())
        .form(&[
            ("_csrf", csrf),
            ("name", "HTTP created profile"),
            ("engine", "postgres"),
            ("host", app.engine.connection.host.as_str()),
            ("port", port.as_str()),
            ("database", app.engine.connection.database.as_str()),
            ("username", app.engine.connection.user.as_str()),
            ("password", ""),
            ("ssl_mode", "disable"),
            ("ssl_root_cert_pem", ""),
            ("statement_timeout_ms", ""),
            ("color", "#0ea5e9"),
        ])
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        response.text()
    );
    let cookie_header = response.cookies().get("gluon_sid").map_or_else(
        || cookie.to_owned(),
        |cookie| format!("gluon_sid={}", cookie.value()),
    );
    let profiles = app.profiles.list(Some(LOCAL_OWNER), u32::MAX).await?;
    assert!(
        profiles
            .iter()
            .any(|profile| profile.name == "HTTP created profile")
    );
    Ok(cookie_header)
}

async fn test_connection_fragment(app: &TestApp, csrf: &str, cookie: &str) -> TestResult<String> {
    let Authentication::Password { password } = &app.engine.connection.auth else {
        return Err(
            std::io::Error::other("PostgreSQL testkit requires password authentication").into(),
        );
    };
    let port = app.engine.connection.port.to_string();
    let response = app
        .client
        .server()
        .post("/connections/test")
        .add_header("cookie", cookie.to_owned())
        .add_header("x-csrf-token", csrf.to_owned())
        .add_header("hx-request", "true")
        .form(&[
            ("_csrf", csrf),
            ("name", "HTTP connection test"),
            ("engine", "postgres"),
            ("host", app.engine.connection.host.as_str()),
            ("port", port.as_str()),
            ("database", app.engine.connection.database.as_str()),
            ("username", app.engine.connection.user.as_str()),
            ("password", password.as_str()),
            ("ssl_mode", "disable"),
            ("ssl_root_cert_pem", ""),
            ("statement_timeout_ms", ""),
            ("color", "#0ea5e9"),
        ])
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "{}",
        response.text()
    );
    assert!(response.text().contains("Connection successful"));
    assert!(
        response
            .text()
            .contains(app.engine.connection.database.as_str())
    );
    let empty_password = app
        .client
        .server()
        .post("/connections/test")
        .add_header("cookie", cookie.to_owned())
        .add_header("x-csrf-token", csrf.to_owned())
        .add_header("hx-request", "true")
        .form(&[
            ("_csrf", csrf),
            ("name", "HTTP empty-password test"),
            ("engine", "postgres"),
            ("host", app.engine.connection.host.as_str()),
            ("port", port.as_str()),
            ("database", app.engine.connection.database.as_str()),
            ("username", app.engine.connection.user.as_str()),
            ("password", ""),
            ("ssl_mode", "disable"),
            ("ssl_root_cert_pem", ""),
            ("statement_timeout_ms", ""),
            ("color", "#0ea5e9"),
        ])
        .await;
    assert_eq!(
        empty_password.status_code(),
        StatusCode::OK,
        "{}",
        empty_password.text()
    );
    assert!(
        !empty_password
            .text()
            .contains("A decrypted password is required to connect.")
    );
    Ok(response.cookies().get("gluon_sid").map_or_else(
        || cookie.to_owned(),
        |cookie| format!("gluon_sid={}", cookie.value()),
    ))
}
