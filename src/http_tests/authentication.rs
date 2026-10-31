use std::sync::Arc;

use async_trait::async_trait;
use axum::http::StatusCode;
use gluon::{
    ContainerBuilder,
    auth::{
        Auth, AuthBuilder, AuthConfig, AuthError, AuthStore, MemoryAuthStore, OAuthExchange,
        OAuthIdentity, OAuthProvider, RegistrationPolicy, UserId, VerifiedIdentity,
    },
    testing::TestClient,
};
use pgpaint::{
    auth::AuthUi,
    domain::{
        connection_profile::{
            ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId,
            ConnectionProfileRepository,
        },
        database::{Engine, OwnerId, QueryId, SslMode},
        query_history::{QueryHistory, QueryHistoryId, QueryHistoryRepository, QueryHistoryStatus},
        query_run::{QueryRun, QueryRunStatus, QueryRunStore},
        saved_query::{SavedQuery, SavedQueryId, SavedQueryRepository},
    },
    usecases::open_connection::{OpenConnection, OpenConnectionImpl},
};
use pgpaint_testkit::{Database, metadata_postgres};
use sqlx::{PgPool, postgres::PgPoolOptions};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{FakeSecretCipher, TestResult};

const ALICE: &str = "user-alice";
const BOB: &str = "user-bob";

struct TestProvider;

#[async_trait]
impl OAuthProvider for TestProvider {
    fn name(&self) -> &'static str {
        "test"
    }

    fn callback_url(&self) -> Option<&str> {
        Some("https://app.example.test/oauth/test/callback")
    }

    fn authorization_url(&self, state: &str, _challenge: &str, _nonce: &str) -> String {
        format!("https://dex.example.test/authorize?state={state}")
    }

    async fn exchange(&self, request: OAuthExchange) -> Result<OAuthIdentity, AuthError> {
        Ok(OAuthIdentity {
            issuer: "https://dex.example.test".to_owned(),
            subject: request.code,
            email: None,
        })
    }
}
struct TestRegistrationPolicy;

impl RegistrationPolicy for TestRegistrationPolicy {
    fn register(&self, identity: &VerifiedIdentity) -> Result<Option<UserId>, AuthError> {
        Ok(Some(UserId(identity.subject.clone())))
    }
}

struct AuthTestApp {
    client: TestClient,
    _metadata: Database,
    profiles: Arc<dyn ConnectionProfileRepository>,
    histories: Arc<dyn QueryHistoryRepository>,
    saved: Arc<dyn SavedQueryRepository>,
    runs: Arc<dyn QueryRunStore>,
}

async fn new_auth_test_app() -> TestResult<AuthTestApp> {
    let metadata = metadata_postgres().await?;
    let pool = Arc::new(
        PgPoolOptions::new()
            .connect(&metadata.connection.database_url())
            .await?,
    );
    crate::MIGRATOR.run(pool.as_ref()).await?;

    let mut auth_config: AuthConfig = AuthBuilder::new()
        .store(Arc::new(MemoryAuthStore::new(10_000)) as Arc<dyn AuthStore>)
        .build()?;
    auth_config.allowed_origins = vec!["https://app.example.test".to_owned()];
    auth_config.oauth.push(Arc::new(TestProvider));
    let auth = Arc::new(
        Auth::new(auth_config, None)
            .await?
            .with_registration_policy(Arc::new(TestRegistrationPolicy)),
    );
    let auth_ui = Arc::new(AuthUi {
        email_otp: false,
        passkeys: false,
        oauth_providers: vec!["test".to_owned()],
    });

    let builder =
        crate::wiring::build_container(ContainerBuilder::new().bind_instance::<PgPool>(pool))
            .bind_instance(auth.clone())
            .bind_instance(auth_ui);
    let mut container = builder.build();
    let profiles = container.resolve::<dyn ConnectionProfileRepository>();
    let histories = container.resolve::<dyn QueryHistoryRepository>();
    let saved = container.resolve::<dyn SavedQueryRepository>();
    let runs = container.resolve::<dyn QueryRunStore>();
    let gateway = container.resolve::<dyn pgpaint::domain::database::DatabaseGateway>();
    container.override_with::<dyn OpenConnection>(Arc::new(OpenConnectionImpl::new(
        Arc::clone(&profiles),
        Arc::new(FakeSecretCipher),
        gateway,
    )));

    let router = crate::generated_app::__gluon_router()
        .nest("/auth", auth.router())
        .layer(axum::middleware::from_fn(
            gluon::middleware::csrf::csrf_middleware,
        ))
        .layer(
            tower_sessions::SessionManagerLayer::new(tower_sessions::MemoryStore::default())
                .with_name("gluon_sid"),
        );
    let client = TestClient::new(router, container)?;
    Ok(AuthTestApp {
        client,
        _metadata: metadata,
        profiles,
        histories,
        saved,
        runs,
    })
}

async fn login(client: &TestClient, subject: &str) -> TestResult<String> {
    let csrf_response = client.server().get("/auth/csrf").await;
    assert_eq!(csrf_response.status_code(), StatusCode::OK);
    let csrf_body: serde_json::Value = serde_json::from_str(&csrf_response.text())?;
    let csrf = csrf_body["token"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("CSRF token missing from auth response"))?;
    let cookie = csrf_response
        .cookies()
        .get("gluon_sid")
        .map(|value| format!("gluon_sid={}", value.value()))
        .ok_or_else(|| std::io::Error::other("auth session cookie missing"))?;

    let started = client
        .server()
        .post("/auth/oauth/test/start")
        .add_header("cookie", cookie.clone())
        .add_header("x-csrf-token", csrf)
        .await;
    assert_eq!(started.status_code(), StatusCode::OK, "{}", started.text());
    let cookie = started.cookies().get("gluon_sid").map_or_else(
        || cookie.clone(),
        |value| format!("gluon_sid={}", value.value()),
    );
    let start_body: serde_json::Value = serde_json::from_str(&started.text())?;
    let state = start_body["flow_id"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("OAuth flow state missing"))?;

    let callback = client
        .server()
        .get(&format!(
            "/oauth/test/callback?state={state}&code={subject}"
        ))
        .add_header("cookie", cookie.clone())
        .await;
    assert_eq!(
        callback.status_code(),
        StatusCode::SEE_OTHER,
        "{}",
        callback.text()
    );
    Ok(callback
        .cookies()
        .get("gluon_sid")
        .map_or_else(|| cookie, |value| format!("gluon_sid={}", value.value())))
}

fn profile(
    id: ConnectionProfileId,
    owner: &str,
    name: &str,
) -> Result<ConnectionProfile, gluon::AppError> {
    ConnectionProfile::new(
        id,
        Some(owner.to_owned()),
        name.to_owned(),
        Engine::Postgres,
        "127.0.0.1".to_owned(),
        5432,
        "postgres".to_owned(),
        "postgres".to_owned(),
        ConnectionProfileAuth::Password {
            encrypted_secret: None,
        },
        SslMode::Disable,
        None,
        false,
        None,
        None,
    )
}

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread")]
async fn auth_redirects_anonymous_pages_and_isolates_owned_data() -> TestResult<()> {
    let app = new_auth_test_app().await?;

    let anonymous_page = app
        .client
        .server()
        .get("/connections/new")
        .add_header("accept", "text/html")
        .await;
    assert_eq!(anonymous_page.status_code(), StatusCode::SEE_OTHER);
    assert_eq!(anonymous_page.headers()["location"], "/login");

    let anonymous_json = app
        .client
        .server()
        .get(&format!("/c/{}/completion", Uuid::new_v4()))
        .add_header("accept", "application/json")
        .await;
    assert_eq!(anonymous_json.status_code(), StatusCode::UNAUTHORIZED);
    let login_page = app.client.server().get("/login").await;
    assert_eq!(
        login_page.status_code(),
        StatusCode::OK,
        "{}",
        login_page.text()
    );
    assert!(login_page.text().contains("Continue with test"));

    let alice_cookie = login(&app.client, ALICE).await?;
    let bob_cookie = login(&app.client, BOB).await?;
    let alice_profile_id = ConnectionProfileId(Uuid::new_v4());
    let bob_profile_id = ConnectionProfileId(Uuid::new_v4());
    app.profiles
        .save(&profile(
            alice_profile_id.clone(),
            ALICE,
            "Alice private profile",
        )?)
        .await?;
    app.profiles
        .save(&profile(bob_profile_id.clone(), BOB, "Bob profile")?)
        .await?;

    let alice_home = app
        .client
        .server()
        .get("/")
        .add_header("cookie", alice_cookie.clone())
        .await;
    assert!(alice_home.text().contains("Alice private profile"));
    let bob_home = app
        .client
        .server()
        .get("/")
        .add_header("cookie", bob_cookie.clone())
        .await;
    assert!(!bob_home.text().contains("Alice private profile"));
    assert!(bob_home.text().contains("Bob profile"));

    let foreign_edit = app
        .client
        .server()
        .get(&format!("/connections/{}/edit", alice_profile_id.0))
        .add_header("cookie", bob_cookie.clone())
        .await;
    assert_eq!(foreign_edit.status_code(), StatusCode::NOT_FOUND);

    let run_id = QueryId(Uuid::new_v4());
    app.runs.insert(QueryRun {
        id: run_id,
        owner: OwnerId(ALICE.to_owned()),
        profile_id: alice_profile_id.clone(),
        sql: "SELECT 1".to_owned(),
        warnings: Vec::new(),
        status: QueryRunStatus::Succeeded,
        result: None,
        error: None,
        elapsed_ms: 1,
        cancel_requested: false,
    })?;
    let foreign_run = app
        .client
        .server()
        .get(&format!("/c/{}/runs/{}", bob_profile_id.0, run_id.0))
        .add_header("cookie", bob_cookie.clone())
        .await;
    assert_eq!(foreign_run.status_code(), StatusCode::NOT_FOUND);

    let history_id = QueryHistoryId(Uuid::new_v4());
    app.histories
        .save(&QueryHistory {
            id: history_id,
            owner: Some(ALICE.to_owned()),
            profile_id: alice_profile_id.clone(),
            sql: "SELECT alice secret history".to_owned(),
            executed_at: OffsetDateTime::UNIX_EPOCH,
            elapsed_ms: 1,
            rows: Some(1),
            rows_affected: None,
            status: QueryHistoryStatus::Succeeded,
            error_message: None,
        })
        .await?;
    let bob_history = app
        .client
        .server()
        .get("/history")
        .add_header("cookie", bob_cookie.clone())
        .await;
    assert_eq!(
        bob_history.status_code(),
        StatusCode::OK,
        "{}",
        bob_history.text()
    );
    assert!(!bob_history.text().contains("alice secret history"));
    let foreign_history = app
        .client
        .server()
        .get(&format!("/c/{}/history", alice_profile_id.0))
        .add_header("cookie", bob_cookie.clone())
        .await;
    assert_eq!(foreign_history.status_code(), StatusCode::NOT_FOUND);

    let saved_id = SavedQueryId(Uuid::new_v4());
    app.saved
        .save(&SavedQuery {
            id: saved_id.clone(),
            owner: Some(ALICE.to_owned()),
            profile_id: Some(alice_profile_id.clone()),
            name: "Alice private saved query".to_owned(),
            sql: "SELECT alice secret saved".to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        })
        .await?;
    let bob_saved = app
        .client
        .server()
        .get("/saved")
        .add_header("cookie", bob_cookie.clone())
        .await;
    assert_eq!(
        bob_saved.status_code(),
        StatusCode::OK,
        "{}",
        bob_saved.text()
    );
    assert!(!bob_saved.text().contains("Alice private saved query"));
    let foreign_saved = app
        .client
        .server()
        .get(&format!("/saved/{}", saved_id.0))
        .add_header("cookie", bob_cookie)
        .await;
    assert_eq!(foreign_saved.status_code(), StatusCode::NOT_FOUND);
    Ok(())
}
