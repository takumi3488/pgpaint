#![doc = include_str!("../README.md")]

use anyhow::{Context, Result, bail};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_dsql::{
    Client as DsqlClient,
    auth_token::{AuthTokenGenerator, Config as AuthTokenConfig},
    config::Region,
    types::ClusterStatus,
};
use bollard::{
    Docker, errors::Error as BollardError, query_parameters::RemoveContainerOptionsBuilder,
};
use rcgen::{BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair};
use sqlx::{
    AssertSqlSafe, Connection, PgConnection, PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};
use std::{
    any::Any,
    net::TcpListener as StdTcpListener,
    path::PathBuf,
    time::{Duration, Instant},
};
use testcontainers_modules::{
    cockroach_db::{CockroachDb, CockroachDbCmd},
    postgres::Postgres,
    testcontainers::{
        CopyTargetOptions, GenericImage, ImageExt,
        core::{IntoContainerPort, Mount},
        runners::AsyncRunner,
    },
};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    task::{JoinHandle, JoinSet},
    time::{sleep, timeout as tokio_timeout},
};
use uuid::Uuid;

const POSTGRES_TAG: &str = "18.6-trixie";
const COCKROACH_TAG: &str = "v25.2.26";
const YUGABYTE_TAG: &str = "2025.2.8.0-b60";
// Stable Floci 2.3.0 rejects DSQL CreateCluster; use a dated nightly until it is released.
const FLOCI_TAG: &str = "nightly-10292026";
const DSQL_REGION: &str = "us-east-1";
const DSQL_ACCESS_KEY: &str = "test";
const DSQL_SECRET_KEY: &str = "test";
const DSQL_DATABASE: &str = "postgres";
const DSQL_USER: &str = "admin";
const CONNECTION_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(5);
const DSQL_API_TIMEOUT: Duration = Duration::from_secs(10);
const DOCKER_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const SEED_STATEMENT_TIMEOUT: Duration = Duration::from_secs(120);
const DSQL_CLEANUP_TIMEOUT: Duration = Duration::from_secs(75);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatabaseKind {
    Metadata,
    Postgres,
    Cockroach,
    Yugabyte,
    Dsql,
}

impl std::fmt::Display for DatabaseKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Metadata => "metadata PostgreSQL",
            Self::Postgres => "PostgreSQL",
            Self::Cockroach => "CockroachDB",
            Self::Yugabyte => "YugabyteDB",
            Self::Dsql => "Aurora DSQL (Floci)",
        })
    }
}

#[derive(Clone)]
pub enum Authentication {
    Password {
        password: String,
    },
    DsqlIam {
        region: String,
        control_plane_endpoint: String,
        access_key_id: String,
        secret_access_key: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TlsMode {
    Disabled,
    Require,
    VerifyFull,
}

#[derive(Clone)]
pub struct TlsInfo {
    pub mode: TlsMode,
    pub ca_certificate_pem: Option<String>,
}

#[derive(Clone)]
pub struct ConnectionParams {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub auth: Authentication,
    pub tls: Option<TlsInfo>,
}

impl ConnectionParams {
    /// Returns a PostgreSQL URL; DSQL uses a short-lived IAM password generated at connection time.
    #[must_use]
    pub fn database_url(&self) -> String {
        let tls = if self.tls.is_some() {
            "?sslmode=verify-full"
        } else {
            ""
        };
        match &self.auth {
            Authentication::Password { password } => format!(
                "postgresql://{}:{}@{}:{}/{}{}",
                self.user, password, self.host, self.port, self.database, tls
            ),
            Authentication::DsqlIam { .. } => format!(
                "postgresql://{}@{}:{}/{}{}",
                self.user, self.host, self.port, self.database, tls
            ),
        }
    }
}

struct TcpForwarder {
    port: u16,
    task: JoinHandle<()>,
}

impl TcpForwarder {
    async fn start(target_host: String, target_port: u16) -> Result<Self> {
        let listener = tokio_timeout(
            CONNECTION_ATTEMPT_TIMEOUT,
            TcpListener::bind(("127.0.0.1", 0)),
        )
        .await
        .context("timed out binding local DSQL TCP forwarder")??;
        let port = listener.local_addr()?.port();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((mut client, _)) = accepted else {
                            break;
                        };
                        let target_host = target_host.clone();
                        connections.spawn(async move {
                            if let Ok(Ok(mut server)) = tokio_timeout(
                                CONNECTION_ATTEMPT_TIMEOUT,
                                TcpStream::connect((target_host.as_str(), target_port)),
                            )
                            .await
                            {
                                let _ = copy_bidirectional(&mut client, &mut server).await;
                            }
                        });
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Ok(Self { port, task })
    }
}

impl Drop for TcpForwarder {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct DsqlCleanup {
    client: DsqlClient,
    docker: Docker,
    control_plane_endpoint: String,
    identifier: String,
    armed: bool,
}

impl DsqlCleanup {
    async fn shutdown(&mut self) -> Result<()> {
        let result = cleanup_dsql_cluster(&self.client, &self.docker, &self.identifier).await;
        if result.is_ok() {
            self.armed = false;
        }
        result
    }
}

impl Drop for DsqlCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let control_plane_endpoint = self.control_plane_endpoint.clone();
        let identifier = self.identifier.clone();
        let cleanup_identifier = identifier.clone();
        let task = std::thread::Builder::new().spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async move {
                // Don't reuse clients whose pooled I/O may depend on the runtime blocked by Drop.
                let docker = Docker::connect_with_local_defaults()
                    .context("connecting to Docker API for DSQL cleanup")?;
                let client = dsql_client(&control_plane_endpoint);
                tokio_timeout(
                    DSQL_CLEANUP_TIMEOUT,
                    cleanup_dsql_cluster(&client, &docker, &cleanup_identifier),
                )
                .await
                .context("timed out cleaning up Floci DSQL cluster")
                .and_then(|result| result)
            })
        });
        match task {
            Ok(task) => match task.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    eprintln!("failed cleaning up Floci DSQL cluster {identifier}: {error:#}");
                }
                Err(_) => eprintln!("DSQL cleanup thread panicked for cluster {identifier}"),
            },
            Err(error) => {
                eprintln!("failed to start DSQL cleanup thread for cluster {identifier}: {error}");
            }
        }
    }
}

async fn cleanup_dsql_cluster(
    client: &DsqlClient,
    docker: &Docker,
    identifier: &str,
) -> Result<()> {
    let child_name = format!("floci-aws-dsql-{identifier}");
    let delete_error = match tokio_timeout(
        DSQL_API_TIMEOUT,
        client.delete_cluster().identifier(identifier).send(),
    )
    .await
    {
        Ok(Ok(_)) => None,
        Ok(Err(error)) => Some(error.to_string()),
        Err(_) => Some(format!("timed out after {DSQL_API_TIMEOUT:?}")),
    };
    let first_wait_error = match wait_for_child_container_removal(
        docker,
        &child_name,
        Duration::from_secs(30),
    )
    .await
    {
        Ok(true) => return Ok(()),
        Ok(false) => None,
        Err(error) => Some(error.to_string()),
    };

    let options = RemoveContainerOptionsBuilder::default()
        .force(true)
        .v(true)
        .build();
    match tokio_timeout(
        DOCKER_REQUEST_TIMEOUT,
        docker.remove_container(&child_name, Some(options)),
    )
    .await
    {
        Ok(Ok(()))
        | Ok(Err(BollardError::DockerResponseServerError {
            status_code: 404, ..
        })) => {}
        Ok(Err(error)) => {
            return Err(error)
                .with_context(|| format!("removing DSQL child container {child_name}"));
        }
        Err(_) => bail!(
            "timed out removing DSQL child container {child_name} after {DOCKER_REQUEST_TIMEOUT:?}"
        ),
    }
    if wait_for_child_container_removal(docker, &child_name, Duration::from_secs(10)).await? {
        return Ok(());
    }
    bail!(
        "DSQL child container {child_name} still exists after forced removal (DeleteCluster error: {}; initial removal check error: {})",
        delete_error.as_deref().unwrap_or("none"),
        first_wait_error.as_deref().unwrap_or("none")
    )
}

async fn wait_for_child_container_removal(
    docker: &Docker,
    name: &str,
    timeout: Duration,
) -> Result<bool> {
    let deadline = Instant::now() + timeout;
    loop {
        match tokio_timeout(
            DOCKER_REQUEST_TIMEOUT,
            docker.inspect_container(name, None),
        )
        .await
        .with_context(|| {
            format!("timed out inspecting DSQL child container {name} after {DOCKER_REQUEST_TIMEOUT:?}")
        })?
        {
            Ok(_) => {}
            Err(BollardError::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                return Ok(true);
            }
            Err(error) => {
                return Err(error).with_context(|| format!("checking DSQL child container {name}"));
            }
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        sleep(Duration::from_millis(250)).await;
    }
}

/// Owns a running database container and the parameters needed to connect to it.
pub struct Database {
    _dsql_cleanup: Option<DsqlCleanup>,
    _forwarder: Option<TcpForwarder>,
    _container: Box<dyn Any + Send>,
    pub kind: DatabaseKind,
    pub connection: ConnectionParams,
}

impl Database {
    fn new<C: Any + Send>(
        container: C,
        kind: DatabaseKind,
        connection: ConnectionParams,
        forwarder: Option<TcpForwarder>,
        dsql_cleanup: Option<DsqlCleanup>,
    ) -> Self {
        Self {
            _dsql_cleanup: dsql_cleanup,
            _forwarder: forwarder,
            _container: Box::new(container),
            kind,
            connection,
        }
    }

    /// Shuts down this database and awaits Floci DSQL cleanup when applicable.
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(mut cleanup) = self._dsql_cleanup.take() {
            cleanup.shutdown().await?;
        }
        Ok(())
    }
}

struct TempDirectory(PathBuf);

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Owns a disposable Dex OpenID Connect provider configured for local browser tests.
pub struct Dex {
    _container: Box<dyn Any + Send>,
    _files: TempDirectory,
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub username: String,
    pub email: String,
    pub password: String,
    pub ca_certificate_pem: String,
    pub ca_certificate_path: PathBuf,
    pub https_certificate_pem: String,
    pub https_private_key_pem: String,
}

/// Starts a local HTTPS Dex provider with a generated test CA and a static user.
///
/// The callback URI must exactly match the URI registered with the app. Configure
/// the returned CA PEM in the OIDC client; never expose this test provider publicly.
pub async fn dex(redirect_uri: &str) -> Result<Dex> {
    if !redirect_uri.chars().all(|character| {
        character.is_ascii_graphic() && character != '\'' && character != '"' && character != '\\'
    }) {
        bail!("Dex callback URI must be a printable URL");
    }
    let redirect_uri = reqwest::Url::parse(redirect_uri).context("parsing Dex callback URI")?;
    if !matches!(redirect_uri.scheme(), "http" | "https")
        || !redirect_uri.username().is_empty()
        || redirect_uri.password().is_some()
    {
        bail!("Dex callback URI must use HTTP or HTTPS without credentials");
    }

    let host_port = StdTcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    let host = testcontainers_host_override().unwrap_or_else(|| "127.0.0.1".to_owned());
    let issuer_url = format!("https://{host}:{host_port}");
    let (ca_certificate_pem, server_certificate_pem, server_key_pem) =
        generate_test_certificate(&host)?;
    let client_id = "pgpaint-test-client".to_owned();
    let client_secret = "pgpaint-test-secret".to_owned();
    let username = "dex-user".to_owned();
    let email = "dex-user@example.test".to_owned();
    let password = "password".to_owned();
    let config = format!(
        r#"issuer: '{issuer_url}'
storage:
  type: memory
web:
  https: 0.0.0.0:5554
  tlsCert: /etc/dex/tls.crt
  tlsKey: /etc/dex/tls.key
oauth2:
  skipApprovalScreen: true
staticClients:
  - id: '{client_id}'
    redirectURIs:
      - '{redirect_uri}'
    name: 'pgpaint test client'
    secret: '{client_secret}'
enablePasswordDB: true
staticPasswords:
  - email: '{email}'
    hash: '$2a$10$ZNmbqC3fe2zpgnZPvE4THuHFfbBPa./K/ULfMzMWzHEHLJufTBA/a'
    username: '{username}'
    userID: 'pgpaint-dex-test-user'
"#,
        redirect_uri = redirect_uri.as_str(),
        email = email.as_str(),
    );

    let files_path = std::env::temp_dir().join(format!("pgpaint-dex-{}", Uuid::new_v4()));
    std::fs::create_dir(&files_path)?;
    let files = TempDirectory(files_path.clone());
    let ca_certificate_path = files_path.join("ca.pem");
    write_private_file(&ca_certificate_path, &ca_certificate_pem)?;

    let container = GenericImage::new("ghcr.io/dexidp/dex", "v2.41.1")
        .with_mapped_port(host_port, 5554.tcp())
        .with_startup_timeout(Duration::from_secs(120))
        .with_copy_to("/etc/dex/config.yaml", config.into_bytes())
        .with_copy_to(
            "/etc/dex/tls.crt",
            server_certificate_pem.clone().into_bytes(),
        )
        .with_copy_to(
            CopyTargetOptions::new("/etc/dex/tls.key").with_mode(0o644),
            server_key_pem.clone().into_bytes(),
        )
        .with_cmd(["dex", "serve", "/etc/dex/config.yaml"])
        .start()
        .await
        .context("starting Dex test provider")?;
    if let Err(error) = wait_for_dex(
        &format!("{issuer_url}/healthz"),
        &ca_certificate_pem,
        Duration::from_secs(120),
    )
    .await
    {
        let stdout = tokio_timeout(DOCKER_REQUEST_TIMEOUT, container.stdout_to_vec())
            .await
            .context("timed out collecting Dex container stdout")?
            .unwrap_or_default();
        let stderr = tokio_timeout(DOCKER_REQUEST_TIMEOUT, container.stderr_to_vec())
            .await
            .context("timed out collecting Dex container stderr")?
            .unwrap_or_default();
        return Err(error.context(format!(
            "Dex container logs:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        )));
    }
    Ok(Dex {
        _container: Box::new(container),
        _files: files,
        issuer_url,
        client_id,
        client_secret,
        username,
        email,
        password,
        ca_certificate_pem,
        ca_certificate_path,
        https_certificate_pem: server_certificate_pem,
        https_private_key_pem: server_key_pem,
    })
}

fn generate_test_certificate(host: &str) -> Result<(String, String, String)> {
    let ca_key = KeyPair::generate()?;
    let mut ca_params = CertificateParams::default();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_certificate = ca_params.self_signed(&ca_key)?;

    let mut server_params = CertificateParams::new(vec![host.to_owned()])?;
    server_params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);
    let server_key = KeyPair::generate()?;
    let server_certificate = server_params.signed_by(&server_key, &ca_certificate, &ca_key)?;
    Ok((
        ca_certificate.pem(),
        server_certificate.pem(),
        server_key.serialize_pem(),
    ))
}

fn write_private_file(path: &std::path::Path, contents: &str) -> Result<()> {
    std::fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

async fn wait_for_dex(url: &str, ca_pem: &str, timeout: Duration) -> Result<()> {
    let root_certificate = reqwest::Certificate::from_pem(ca_pem.as_bytes())?;
    let client = reqwest::Client::builder()
        .add_root_certificate(root_certificate)
        .timeout(Duration::from_secs(2))
        .build()?;
    let deadline = Instant::now() + timeout;
    loop {
        let last_error = match client.get(url).send().await {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => format!("health check returned {}", response.status()),
            Err(error) => format!("{error:?}"),
        };
        if Instant::now() >= deadline {
            bail!("timed out waiting for Dex at {url}: {last_error}");
        }
        sleep(Duration::from_secs(1)).await;
    }
}

/// Starts PostgreSQL for pgpaint's own metadata, auth, and session tables.
pub async fn metadata_postgres() -> Result<Database> {
    start_postgres("pgpaint", "pgpaint", "pgpaint", DatabaseKind::Metadata).await
}

/// Starts the latest stable PostgreSQL major release.
pub async fn postgres() -> Result<Database> {
    start_postgres("postgres", "postgres", "postgres", DatabaseKind::Postgres).await
}

fn testcontainers_host_override() -> Option<String> {
    std::env::var("PGPAINT_TESTCONTAINERS_HOST")
        .ok()
        .filter(|host| !host.trim().is_empty())
}

fn connection_host(container_host: String) -> String {
    testcontainers_host_override().unwrap_or(container_host)
}

async fn start_postgres(
    database: &str,
    user: &str,
    password: &str,
    kind: DatabaseKind,
) -> Result<Database> {
    let container = Postgres::default()
        .with_db_name(database)
        .with_user(user)
        .with_password(password)
        .with_tag(POSTGRES_TAG)
        .with_mapped_port(0, 5432.tcp())
        .with_startup_timeout(Duration::from_secs(120))
        .start()
        .await
        .context("starting PostgreSQL container")?;
    let host = tokio_timeout(DOCKER_REQUEST_TIMEOUT, container.get_host())
        .await
        .context("timed out getting PostgreSQL container host")??
        .to_string();
    let port = tokio_timeout(
        DOCKER_REQUEST_TIMEOUT,
        container.get_host_port_ipv4(5432.tcp()),
    )
    .await
    .context("timed out getting PostgreSQL container port")??;
    let connection = ConnectionParams {
        host: connection_host(host),
        port,
        database: database.to_owned(),
        user: user.to_owned(),
        auth: Authentication::Password {
            password: password.to_owned(),
        },
        tls: None,
    };
    wait_for_postgres(&connection, Duration::from_secs(20)).await?;
    Ok(Database::new(container, kind, connection, None, None))
}

/// Starts one insecure CockroachDB node for local development and tests.
pub async fn cockroach() -> Result<Database> {
    let container = CockroachDb::new(CockroachDbCmd::StartSingleNode { insecure: true })
        .with_tag(COCKROACH_TAG)
        .with_mapped_port(0, 26257.tcp())
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .context("starting CockroachDB container")?;
    let host = tokio_timeout(DOCKER_REQUEST_TIMEOUT, container.get_host())
        .await
        .context("timed out getting CockroachDB container host")??
        .to_string();
    let port = tokio_timeout(
        DOCKER_REQUEST_TIMEOUT,
        container.get_host_port_ipv4(26257.tcp()),
    )
    .await
    .context("timed out getting CockroachDB container port")??;
    let connection = ConnectionParams {
        host: connection_host(host),
        port,
        database: "defaultdb".to_owned(),
        user: "root".to_owned(),
        auth: Authentication::Password {
            password: String::new(),
        },
        tls: None,
    };
    wait_for_postgres(&connection, Duration::from_secs(180)).await?;
    Ok(Database::new(
        container,
        DatabaseKind::Cockroach,
        connection,
        None,
        None,
    ))
}

/// Starts a single-node YugabyteDB YSQL instance.
pub async fn yugabyte() -> Result<Database> {
    let container = GenericImage::new("yugabytedb/yugabyte", YUGABYTE_TAG)
        .with_exposed_port(5433.tcp())
        .with_startup_timeout(Duration::from_secs(600))
        .with_cmd(["bin/yugabyted", "start", "--background=false"])
        .start()
        .await
        .context("starting YugabyteDB container")?;
    let host = tokio_timeout(DOCKER_REQUEST_TIMEOUT, container.get_host())
        .await
        .context("timed out getting YugabyteDB container host")??
        .to_string();
    let port = tokio_timeout(
        DOCKER_REQUEST_TIMEOUT,
        container.get_host_port_ipv4(5433.tcp()),
    )
    .await
    .context("timed out getting YugabyteDB container port")??;
    let connection = ConnectionParams {
        host: connection_host(host),
        port,
        database: "yugabyte".to_owned(),
        user: "yugabyte".to_owned(),
        auth: Authentication::Password {
            password: String::new(),
        },
        tls: None,
    };
    wait_for_postgres(&connection, Duration::from_secs(600))
        .await
        .context("waiting for YugabyteDB YSQL to accept connections")?;
    Ok(Database::new(
        container,
        DatabaseKind::Yugabyte,
        connection,
        None,
        None,
    ))
}

async fn wait_for_floci_health(url: &str, timeout: Duration) -> Result<()> {
    let client = reqwest::Client::new();
    let deadline = Instant::now() + timeout;
    loop {
        let error = match tokio_timeout(Duration::from_secs(2), client.get(url).send()).await {
            Ok(Ok(response)) if response.status().is_success() => return Ok(()),
            Ok(Ok(response)) => format!("HTTP {}", response.status()),
            Ok(Err(error)) => error.to_string(),
            Err(_) => "health request timed out".to_owned(),
        };
        if Instant::now() >= deadline {
            bail!("Floci health endpoint {url} did not become ready: {error}");
        }
        sleep(Duration::from_secs(1)).await;
    }
}

fn dsql_client(control_plane_endpoint: &str) -> DsqlClient {
    let credentials = Credentials::new(
        DSQL_ACCESS_KEY,
        DSQL_SECRET_KEY,
        None,
        None,
        "floci-testkit",
    );
    let config = aws_sdk_dsql::Config::builder()
        .behavior_version_latest()
        .region(Region::new(DSQL_REGION))
        .credentials_provider(credentials)
        .endpoint_url(control_plane_endpoint)
        .build();
    DsqlClient::from_conf(config)
}

/// Starts Floci, creates one Aurora DSQL cluster through its AWS API, and retrieves its CA.
pub async fn dsql() -> Result<Database> {
    let docker = Docker::connect_with_local_defaults()
        .context("connecting to Docker API for DSQL cleanup")?;
    let container = GenericImage::new("floci/floci", FLOCI_TAG)
        .with_exposed_port(4566.tcp())
        .with_exposed_port(5432.tcp())
        .with_mount(Mount::bind_mount(
            "/var/run/docker.sock",
            "/var/run/docker.sock",
        ))
        .with_startup_timeout(Duration::from_secs(120))
        .start()
        .await
        .context("starting Floci container; Docker socket /var/run/docker.sock must be mounted")?;
    let container_host = tokio_timeout(DOCKER_REQUEST_TIMEOUT, container.get_host())
        .await
        .context("timed out getting Floci container host")??
        .to_string();
    let host_override = testcontainers_host_override();
    let host = host_override.clone().unwrap_or(container_host);
    let control_plane_port = tokio_timeout(
        DOCKER_REQUEST_TIMEOUT,
        container.get_host_port_ipv4(4566.tcp()),
    )
    .await
    .context("timed out getting Floci control-plane port")??;
    let database_port = tokio_timeout(
        DOCKER_REQUEST_TIMEOUT,
        container.get_host_port_ipv4(5432.tcp()),
    )
    .await
    .context("timed out getting Floci database port")??;
    let control_plane_endpoint = format!("http://{host}:{control_plane_port}");
    wait_for_floci_health(
        &format!("{control_plane_endpoint}/_floci/health"),
        Duration::from_secs(180),
    )
    .await?;
    let ca_client = reqwest::Client::builder()
        .timeout(DSQL_API_TIMEOUT)
        .build()?;
    let ca_response = tokio_timeout(
        DSQL_API_TIMEOUT,
        ca_client
            .get(format!("{control_plane_endpoint}/_floci/ca.pem"))
            .send(),
    )
    .await
    .context("timed out fetching Floci's DSQL CA")?
    .context("fetching Floci's DSQL CA")?
    .error_for_status()
    .context("fetching Floci's DSQL CA")?;
    let ca_certificate_pem = tokio_timeout(DSQL_API_TIMEOUT, ca_response.text())
        .await
        .context("timed out reading Floci's DSQL CA")?
        .context("reading Floci's DSQL CA")?;

    let client = dsql_client(&control_plane_endpoint);
    let created = tokio_timeout(
        DSQL_API_TIMEOUT,
        client
            .create_cluster()
            .deletion_protection_enabled(false)
            .send(),
    )
    .await
    .context("timed out creating Floci DSQL cluster")?
    .context("creating Floci DSQL cluster")?;
    let identifier = created.identifier().to_owned();
    let mut cleanup = DsqlCleanup {
        client: client.clone(),
        docker,
        control_plane_endpoint: control_plane_endpoint.clone(),
        identifier: identifier.clone(),
        armed: true,
    };
    let endpoint = match wait_for_dsql_cluster(&client, &identifier).await {
        Ok(endpoint) => endpoint,
        Err(error) => {
            cleanup
                .shutdown()
                .await
                .context("cleaning up failed DSQL startup")?;
            return Err(error);
        }
    };
    let forwarder = match host_override {
        Some(host) => match TcpForwarder::start(host, database_port).await {
            Ok(forwarder) => Some(forwarder),
            Err(error) => {
                cleanup
                    .shutdown()
                    .await
                    .context("cleaning up failed DSQL forwarder startup")?;
                return Err(error);
            }
        },
        None => None,
    };
    let connection_port = forwarder
        .as_ref()
        .map_or(database_port, |forwarder| forwarder.port);

    let connection = ConnectionParams {
        host: endpoint,
        port: connection_port,
        database: DSQL_DATABASE.to_owned(),
        user: DSQL_USER.to_owned(),
        auth: Authentication::DsqlIam {
            region: DSQL_REGION.to_owned(),
            control_plane_endpoint,
            access_key_id: DSQL_ACCESS_KEY.to_owned(),
            secret_access_key: DSQL_SECRET_KEY.to_owned(),
        },
        tls: Some(TlsInfo {
            mode: TlsMode::VerifyFull,
            ca_certificate_pem: Some(ca_certificate_pem),
        }),
    };
    Ok(Database::new(
        container,
        DatabaseKind::Dsql,
        connection,
        forwarder,
        Some(cleanup),
    ))
}

async fn wait_for_dsql_cluster(client: &DsqlClient, identifier: &str) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let cluster = tokio_timeout(
            DSQL_API_TIMEOUT,
            client.get_cluster().identifier(identifier).send(),
        )
        .await
        .context("timed out waiting for Floci DSQL cluster")?
        .context("waiting for Floci DSQL cluster")?;
        if cluster.status() == &ClusterStatus::Active {
            return cluster
                .endpoint()
                .map(|endpoint| endpoint.trim_end_matches(":5432").to_owned())
                .context("Floci marked the DSQL cluster active without an endpoint");
        }
        if cluster.status() == &ClusterStatus::Failed {
            bail!("Floci DSQL cluster {identifier} failed to start");
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for Floci DSQL cluster {identifier} (status: {})",
                cluster.status()
            );
        }
        sleep(Duration::from_secs(1)).await;
    }
}

async fn connection_options(connection: &ConnectionParams) -> Result<PgConnectOptions> {
    let mut options = PgConnectOptions::new()
        .host(&connection.host)
        .port(connection.port)
        .database(&connection.database)
        .username(&connection.user)
        .ssl_mode(if connection.tls.is_some() {
            PgSslMode::Require
        } else {
            PgSslMode::Disable
        });
    match &connection.auth {
        Authentication::Password { password } => {
            options = options.password(password);
        }
        Authentication::DsqlIam {
            region,
            access_key_id,
            secret_access_key,
            ..
        } => {
            let credentials = Credentials::new(
                access_key_id,
                secret_access_key,
                None,
                None,
                "floci-testkit",
            );
            let aws_region = Region::new(region.clone());
            let shared_config = aws_config::defaults(BehaviorVersion::latest())
                .region(aws_region.clone())
                .credentials_provider(credentials.clone())
                .load()
                .await;
            let auth_config = AuthTokenConfig::builder()
                .hostname(&connection.host)
                .region(aws_region)
                .credentials(credentials)
                .build()
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let token = AuthTokenGenerator::new(auth_config)
                .db_connect_admin_auth_token(&shared_config)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            options = options.password(token.as_str());
        }
    }
    Ok(options)
}

async fn wait_for_postgres(connection: &ConnectionParams, timeout: Duration) -> Result<()> {
    let options = tokio_timeout(DSQL_API_TIMEOUT, connection_options(connection))
        .await
        .context("timed out preparing PostgreSQL readiness connection")??;
    let deadline = Instant::now() + timeout;
    let mut last_error = "no connection attempt completed".to_owned();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!(
                "{}:{} did not accept PostgreSQL connections within {timeout:?}: {last_error}",
                connection.host,
                connection.port
            );
        }
        let attempt_timeout = CONNECTION_ATTEMPT_TIMEOUT.min(remaining);
        match tokio_timeout(attempt_timeout, PgConnection::connect_with(&options)).await {
            Ok(Ok(connection)) => {
                match tokio_timeout(CONNECTION_ATTEMPT_TIMEOUT, connection.close()).await {
                    Ok(Ok(())) => return Ok(()),
                    Ok(Err(error)) => return Err(error).context("closing readiness connection"),
                    Err(_) => bail!(
                        "timed out closing PostgreSQL readiness connection after {CONNECTION_ATTEMPT_TIMEOUT:?}"
                    ),
                }
            }
            Ok(Err(error)) => last_error = error.to_string(),
            Err(_) => {
                last_error = format!("connection attempt timed out after {attempt_timeout:?}")
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            continue;
        }
        sleep(Duration::from_secs(2).min(remaining)).await;
    }
}

/// Applies the engine-specific schema and deterministic seed data to a started database.
pub async fn seed(database: &Database) -> Result<()> {
    let script = match database.kind {
        DatabaseKind::Postgres => include_str!("../../../testdata/seed/postgres.sql"),
        DatabaseKind::Cockroach => include_str!("../../../testdata/seed/cockroach.sql"),
        DatabaseKind::Yugabyte => include_str!("../../../testdata/seed/yugabyte.sql"),
        DatabaseKind::Dsql => include_str!("../../../testdata/seed/dsql.sql"),
        DatabaseKind::Metadata => bail!("metadata PostgreSQL is not an engine seed target"),
    };
    let options = tokio_timeout(DSQL_API_TIMEOUT, connection_options(&database.connection))
        .await
        .context("timed out preparing seed database connection")??;
    let pool = tokio_timeout(
        Duration::from_secs(30),
        PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(CONNECTION_ATTEMPT_TIMEOUT)
            .connect_with(options),
    )
    .await
    .with_context(|| format!("timed out connecting to {} for seed data", database.kind))?
    .with_context(|| format!("connecting to {} for seed data", database.kind))?;
    let result = apply_seed(&pool, database.kind, script).await;
    if tokio_timeout(Duration::from_secs(10), pool.close())
        .await
        .is_err()
    {
        bail!(
            "timed out closing the {} seed connection pool",
            database.kind
        );
    }
    result.with_context(|| format!("seeding {}", database.kind))
}

async fn apply_seed(pool: &PgPool, kind: DatabaseKind, script: &str) -> Result<()> {
    for statement in script.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        if kind == DatabaseKind::Dsql
            && (statement.starts_with("CREATE INDEX ASYNC")
                || statement.starts_with("CREATE UNIQUE INDEX ASYNC"))
        {
            let row = tokio_timeout(
                SEED_STATEMENT_TIMEOUT,
                sqlx::query(AssertSqlSafe(statement)).fetch_one(pool),
            )
            .await
            .with_context(|| {
                format!("timed out executing DSQL async index after {SEED_STATEMENT_TIMEOUT:?}")
            })?
            .with_context(|| format!("executing DSQL async index: {statement}"))?;
            let job_id = row
                .try_get::<String, _>(0)
                .or_else(|_| row.try_get::<Uuid, _>(0).map(|id| id.to_string()))?;
            tokio_timeout(
                SEED_STATEMENT_TIMEOUT,
                sqlx::query("CALL sys.wait_for_job($1)")
                    .bind(job_id)
                    .execute(pool),
            )
            .await
            .context("timed out waiting for DSQL async index job")?
            .context("waiting for DSQL async index job")?;
        } else {
            tokio_timeout(
                SEED_STATEMENT_TIMEOUT,
                sqlx::query(AssertSqlSafe(statement)).execute(pool),
            )
            .await
            .with_context(|| {
                format!("timed out executing seed statement after {SEED_STATEMENT_TIMEOUT:?}")
            })?
            .with_context(|| format!("executing seed statement: {statement}"))?;
        }
    }
    Ok(())
}
