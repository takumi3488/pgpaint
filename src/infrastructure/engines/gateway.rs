//! `SqlxGateway`: pooled connections and query operations for PostgreSQL-wire engines.

use std::{
    collections::HashMap,
    fmt::Write as _,
    ops::{Deref, DerefMut},
    sync::{
        Arc, Mutex as StdMutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use futures::{TryStream, TryStreamExt, stream::BoxStream};
use sqlx::{
    AssertSqlSafe, Column as SqlxColumn, Either, PgConnection, PgPool, Postgres, QueryBuilder, Row,
    TypeInfo, ValueRef,
    pool::PoolConnection,
    postgres::{
        PgConnectOptions, PgDatabaseError, PgErrorPosition, PgPoolOptions, PgQueryResult, PgRow,
        PgSslMode, PgValueFormat,
    },
    raw_sql,
};
use tokio::{sync::oneshot, task::AbortHandle};

use crate::domain::database::model::{Column, CompletionSchema, Relationship};
use crate::domain::database::{
    BrowsePage, BrowseRequest, CancelStrategy, ColumnMeta, ConnectAuth, ConnectTarget,
    ConnectionKey, DatabaseGateway, DbError, DbErrorKind, Engine, ExplainResult, FilterOp, QueryId,
    QueryRequest, QueryResult, RelationRef, RowEdit, RowEvent, SchemaTree, ServerInfo, SslMode,
    StatementClass, StatementResult, TableDetail, ViewScope, split_statements,
};
use crate::infrastructure::engines::{CancelHandle, Dialect, dialect};

const DEFAULT_IDLE_TTL: Duration = Duration::from_mins(30);
const EVICTION_INTERVAL: Duration = Duration::from_mins(1);
const MAX_POOL_CONNECTIONS: u32 = 4;
const DSQL_MAX_LIFETIME: Duration = Duration::from_mins(55);
const MAX_ENGINE_VIEW_ROWS: usize = 10_000;
const CANCEL_RETRY_WINDOW: Duration = Duration::from_millis(1_500);
const CANCEL_REQUEST_TIMEOUT: Duration = Duration::from_millis(250);
const CANCEL_RETRY_INTERVAL: Duration = Duration::from_millis(25);
const CANCEL_ABORT_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct SqlxGateway {
    inner: Arc<GatewayInner>,
}

struct GatewayInner {
    pools: RwLock<HashMap<ConnectionKey, Arc<PoolEntry>>>,
    running: StdMutex<HashMap<(ConnectionKey, QueryId), RunningQuery>>,
    idle_ttl: Duration,
    eviction_started: AtomicBool,
}

struct PoolEntry {
    pool: PgPool,
    dialect: &'static dyn Dialect,
    target: ConnectTarget,
    server_info: ServerInfo,
    last_used: StdMutex<Instant>,
    type_names: RwLock<HashMap<u32, String>>,
}

#[derive(Clone)]
struct RunningQuery {
    cancel_handle: CancelHandle,
    abort: AbortHandle,
    close_connection: Arc<AtomicBool>,
    token: Arc<()>,
}

struct RunningQueryCleanup {
    inner: Arc<GatewayInner>,
    key: ConnectionKey,
    id: QueryId,
    token: Arc<()>,
}

struct QueryConnection {
    connection: PoolConnection<Postgres>,
    close_on_drop: Arc<AtomicBool>,
}

impl Deref for QueryConnection {
    type Target = PgConnection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl DerefMut for QueryConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}

impl Drop for QueryConnection {
    fn drop(&mut self) {
        if self.close_on_drop.load(Ordering::Acquire) {
            self.connection.close_on_drop();
        }
    }
}

struct ExportConnection {
    connection: PoolConnection<Postgres>,
    completed: bool,
}

impl Deref for ExportConnection {
    type Target = PgConnection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl DerefMut for ExportConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}

impl Drop for ExportConnection {
    fn drop(&mut self) {
        if !self.completed {
            self.connection.close_on_drop();
        }
    }
}

impl Drop for RunningQueryCleanup {
    fn drop(&mut self) {
        {
            let mut running = self
                .inner
                .running
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let key = (self.key.clone(), self.id);
            if running
                .get(&key)
                .is_some_and(|active| Arc::ptr_eq(&active.token, &self.token))
            {
                running.remove(&key);
            }
        }
    }
}

impl Default for SqlxGateway {
    fn default() -> Self {
        Self::new()
    }
}

impl SqlxGateway {
    #[must_use]
    pub fn new() -> Self {
        let idle_ttl = std::env::var("PGPAINT_CONNECTION_IDLE_TTL_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|seconds| *seconds > 0)
            .map_or(DEFAULT_IDLE_TTL, Duration::from_secs);
        Self::with_idle_ttl(idle_ttl)
    }

    #[must_use]
    pub fn with_idle_ttl(idle_ttl: Duration) -> Self {
        Self {
            inner: Arc::new(GatewayInner {
                pools: RwLock::new(HashMap::new()),
                running: StdMutex::new(HashMap::new()),
                idle_ttl,
                eviction_started: AtomicBool::new(false),
            }),
        }
    }

    fn ensure_eviction_task(&self) {
        if self
            .inner
            .eviction_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let inner = Arc::downgrade(&self.inner);
        let sweep_interval = self
            .inner
            .idle_ttl
            .min(EVICTION_INTERVAL)
            .max(Duration::from_secs(1));
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(sweep_interval).await;
                let Some(inner) = inner.upgrade() else {
                    break;
                };
                let now = Instant::now();
                let expired = {
                    let mut pools = inner
                        .pools
                        .write()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let running = inner
                        .running
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let stale = pools
                        .iter()
                        .filter_map(|(key, entry)| {
                            let idle_since = *entry
                                .last_used
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            let has_running_query =
                                running.keys().any(|(running_key, _)| running_key == key);
                            (now.duration_since(idle_since) >= inner.idle_ttl
                                && !has_running_query
                                && entry.pool.num_idle()
                                    == usize::try_from(entry.pool.size()).unwrap_or(usize::MAX))
                            .then(|| key.clone())
                        })
                        .collect::<Vec<_>>();
                    stale
                        .into_iter()
                        .filter_map(|key| pools.remove(&key))
                        .collect::<Vec<_>>()
                };
                for entry in expired {
                    entry.pool.close().await;
                }
            }
        });
    }

    fn entry(&self, key: &ConnectionKey) -> Result<Arc<PoolEntry>, DbError> {
        let entry = self
            .inner
            .pools
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .cloned()
            .ok_or_else(|| DbError::new(DbErrorKind::NotConnected, "database is not connected"))?;
        entry.touch();
        Ok(entry)
    }

    fn find_entry(&self, key: &ConnectionKey) -> Option<Arc<PoolEntry>> {
        let entry = self
            .inner
            .pools
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .cloned();
        if let Some(entry) = &entry {
            entry.touch();
        }
        entry
    }

    async fn execute_request(
        &self,
        key: &ConnectionKey,
        entry: Arc<PoolEntry>,
        request: QueryRequest,
    ) -> Result<QueryResult, DbError> {
        let started = Instant::now();
        let mut pooled = entry.pool.acquire().await.map_err(map_sqlx_error)?;
        let cancel_handle = entry.dialect.cancel_handle(&mut pooled).await?;
        let close_connection = Arc::new(AtomicBool::new(false));
        let connection = QueryConnection {
            connection: pooled,
            close_on_drop: Arc::clone(&close_connection),
        };
        let query_id = request.id;
        let request_timeout = request.timeout;
        let (start_tx, start_rx) = oneshot::channel();
        let entry_for_task = Arc::clone(&entry);
        let inner_for_task = Arc::clone(&self.inner);
        let task_key = key.clone();
        let token = Arc::new(());
        let cleanup_token = Arc::clone(&token);
        let cleanup = RunningQueryCleanup {
            inner: inner_for_task,
            key: task_key,
            id: query_id,
            token: cleanup_token,
        };
        let task = tokio::spawn(async move {
            let _cleanup = cleanup;
            if start_rx.await.is_err() {
                return Err(DbError::new(
                    DbErrorKind::Canceled,
                    "query was canceled before execution",
                ));
            }
            run_execute(&entry_for_task, connection, request).await
        });
        let abort = task.abort_handle();
        let running_key = (key.clone(), query_id);
        let previous = {
            let mut running = self
                .inner
                .running
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            running.insert(
                running_key,
                RunningQuery {
                    cancel_handle,
                    abort: abort.clone(),
                    close_connection: Arc::clone(&close_connection),
                    token: Arc::clone(&token),
                },
            )
        };
        if let Some(previous) = previous {
            previous.close_connection.store(true, Ordering::Release);
            previous.abort.abort();
        }
        let _ = start_tx.send(());

        let joined = self
            .await_query_task(key, query_id, request_timeout, &close_connection, task)
            .await;
        {
            let mut running = self
                .inner
                .running
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let running_key = (key.clone(), query_id);
            if running
                .get(&running_key)
                .is_some_and(|active| Arc::ptr_eq(&active.token, &token))
            {
                running.remove(&running_key);
            }
        }

        let Some(joined) = joined else {
            return Err(DbError::new(DbErrorKind::Timeout, "query timed out"));
        };
        let statements = match joined {
            Ok(result) => result?,
            Err(error) if error.is_cancelled() => {
                return Err(DbError::new(DbErrorKind::Canceled, "query canceled"));
            }
            Err(error) => {
                return Err(DbError::new(
                    DbErrorKind::Internal,
                    format!("query task failed: {error}"),
                ));
            }
        };
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok(QueryResult {
            statements,
            elapsed_ms,
        })
    }

    async fn await_query_task(
        &self,
        key: &ConnectionKey,
        query_id: QueryId,
        request_timeout: Option<Duration>,
        close_connection: &AtomicBool,
        mut task: tokio::task::JoinHandle<Result<Vec<StatementResult>, DbError>>,
    ) -> Option<Result<Result<Vec<StatementResult>, DbError>, tokio::task::JoinError>> {
        let Some(request_timeout) = request_timeout else {
            return Some(task.await);
        };
        if let Ok(joined) = tokio::time::timeout(request_timeout, &mut task).await {
            return Some(joined);
        }
        let _ = self.cancel(key, query_id).await;
        if tokio::time::timeout(Duration::from_secs(1), &mut task)
            .await
            .is_err()
        {
            close_connection.store(true, Ordering::Release);
            task.abort();
            let _ = task.await;
        }
        None
    }

    async fn cancel_active(&self, key: &ConnectionKey, id: QueryId) -> Result<bool, DbError> {
        let active = self
            .inner
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&(key.clone(), id))
            .cloned();
        let Some(active) = active else {
            return Ok(false);
        };
        if !self.query_is_running(key, id, &active.token) {
            return Ok(true);
        }

        let Some(entry) = self.find_entry(key) else {
            return Ok(self.abort_and_wait(key, id, &active).await);
        };
        if entry.target.engine.capabilities().cancel == CancelStrategy::CloseConnection
            || active.cancel_handle == CancelHandle::CloseConnection
        {
            return Ok(self.abort_and_wait(key, id, &active).await);
        }

        let deadline = Instant::now() + CANCEL_RETRY_WINDOW;
        let mut last_error = None;
        loop {
            if !self.query_is_running(key, id, &active.token) {
                return Ok(true);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(
                CANCEL_REQUEST_TIMEOUT.min(remaining),
                entry.dialect.cancel(&entry.pool, &active.cancel_handle),
            )
            .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => last_error = Some(error),
                Err(_) => {
                    last_error = Some(DbError::new(
                        DbErrorKind::Timeout,
                        "query cancel request timed out",
                    ));
                }
            }
            if !self.query_is_running(key, id, &active.token) {
                return Ok(true);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if !remaining.is_zero() {
                tokio::time::sleep(CANCEL_RETRY_INTERVAL.min(remaining)).await;
            }
        }

        if self.abort_and_wait(key, id, &active).await {
            return Ok(true);
        }
        if let Some(error) = last_error {
            return Err(error);
        }
        Ok(false)
    }

    fn query_is_running(&self, key: &ConnectionKey, id: QueryId, token: &Arc<()>) -> bool {
        self.inner
            .running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&(key.clone(), id))
            .is_some_and(|active| Arc::ptr_eq(&active.token, token))
    }

    async fn abort_and_wait(
        &self,
        key: &ConnectionKey,
        id: QueryId,
        active: &RunningQuery,
    ) -> bool {
        active.close_connection.store(true, Ordering::Release);
        active.abort.abort();
        self.wait_for_query_completion(key, id, active, CANCEL_ABORT_TIMEOUT)
            .await
    }

    async fn wait_for_query_completion(
        &self,
        key: &ConnectionKey,
        id: QueryId,
        active: &RunningQuery,
        timeout: Duration,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if !self.query_is_running(key, id, &active.token) {
                return true;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            tokio::time::sleep(CANCEL_RETRY_INTERVAL.min(remaining)).await;
        }
    }
}

impl PoolEntry {
    fn touch(&self) {
        *self
            .last_used
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Instant::now();
    }
}

fn empty_statement_result() -> StatementResult {
    StatementResult {
        columns: Vec::new(),
        rows: Vec::new(),
        rows_affected: 0,
        truncated: false,
    }
}

pub(super) fn map_sqlx_error(error: sqlx::Error) -> DbError {
    let message = error.to_string();
    match error {
        sqlx::Error::Database(database) => {
            let code = database.code().map(std::borrow::Cow::into_owned);
            let message = database.message().to_owned();
            let kind = match code.as_deref() {
                Some("57014") if message.to_ascii_lowercase().contains("timeout") => {
                    DbErrorKind::Timeout
                }
                Some("57014") => DbErrorKind::Canceled,
                Some("28P01" | "28000" | "28P02") => DbErrorKind::Auth,
                Some(code) if code.starts_with("08") => DbErrorKind::Connect,
                _ => DbErrorKind::Query,
            };
            let postgres_error = database.try_downcast_ref::<PgDatabaseError>();
            let detail = postgres_error
                .and_then(PgDatabaseError::detail)
                .map(ToOwned::to_owned);
            let hint = postgres_error
                .and_then(PgDatabaseError::hint)
                .map(ToOwned::to_owned);
            let position = postgres_error
                .and_then(PgDatabaseError::position)
                .and_then(|position| match position {
                    PgErrorPosition::Original(position) => Some(position),
                    PgErrorPosition::Internal { .. } => None,
                })
                .and_then(|position| u32::try_from(position).ok());
            DbError {
                kind,
                message,
                code,
                detail,
                hint,
                position,
            }
        }
        sqlx::Error::Io(_)
        | sqlx::Error::Tls(_)
        | sqlx::Error::Protocol(_)
        | sqlx::Error::PoolTimedOut
        | sqlx::Error::PoolClosed
        | sqlx::Error::WorkerCrashed => DbError::new(DbErrorKind::Connect, message),
        _ => DbError::new(DbErrorKind::Query, message),
    }
}

fn map_connection_error(error: sqlx::Error) -> DbError {
    let mut mapped = map_sqlx_error(error);
    if mapped.kind == DbErrorKind::Query {
        mapped.kind = DbErrorKind::Connect;
    }
    mapped
}

fn map_connector_error(error: impl std::fmt::Display) -> DbError {
    let message = error.to_string();
    let lowercase = message.to_ascii_lowercase();
    let kind = if lowercase.contains("credential")
        || lowercase.contains("authentication")
        || lowercase.contains("unauthorized")
        || lowercase.contains("access denied")
        || lowercase.contains("accessdenied")
        || lowercase.contains("token")
        || lowercase.contains("signature")
        || lowercase.contains("expired")
    {
        DbErrorKind::Auth
    } else {
        DbErrorKind::Connect
    };
    DbError::new(kind, message)
}

fn target_settings(target: &ConnectTarget) -> PgPoolOptions {
    let read_only = target.read_only;
    let statement_timeout_ms = target.statement_timeout_ms;
    PgPoolOptions::new()
        .max_connections(MAX_POOL_CONNECTIONS)
        .acquire_timeout(Duration::from_secs(15))
        .after_connect(move |connection, _metadata| {
            Box::pin(async move {
                if read_only {
                    raw_sql("SET default_transaction_read_only = on")
                        .execute(&mut *connection)
                        .await?;
                }
                if let Some(timeout_ms) = statement_timeout_ms {
                    let sql = format!("SET statement_timeout = {timeout_ms}");
                    raw_sql(AssertSqlSafe(sql))
                        .execute(&mut *connection)
                        .await?;
                }
                Ok(())
            })
        })
}

pub(super) async fn connect_password_pool(target: &ConnectTarget) -> Result<PgPool, DbError> {
    if target.engine == Engine::AuroraDsql {
        return Err(DbError::new(
            DbErrorKind::Auth,
            "Aurora DSQL requires IAM authentication",
        ));
    }
    let ConnectAuth::Password(password) = &target.auth else {
        return Err(DbError::new(
            DbErrorKind::Auth,
            "password authentication is required for this engine",
        ));
    };
    let mut options = PgConnectOptions::new_without_pgpass()
        .host(&target.host)
        .port(target.port)
        .username(&target.username)
        .password(password.expose())
        .database(&target.database)
        .application_name("pgpaint")
        .ssl_mode(pg_ssl_mode(target.ssl_mode));
    if let Some(pem) = &target.ssl_root_cert_pem {
        options = options.ssl_root_cert_from_pem(pem.as_bytes().to_vec());
    }
    target_settings(target)
        .connect_with(options)
        .await
        .map_err(map_connection_error)
}

pub(super) async fn connect_dsql_pool(target: &ConnectTarget) -> Result<PgPool, DbError> {
    let ConnectAuth::DsqlIam { region, admin } = &target.auth else {
        return Err(DbError::new(
            DbErrorKind::Auth,
            "Aurora DSQL requires IAM authentication",
        ));
    };
    let username = if *admin {
        "admin"
    } else {
        target.username.as_str()
    };
    let mut options = PgConnectOptions::new_without_pgpass()
        .host(&target.host)
        .port(target.port)
        .username(username)
        .database(&target.database)
        .application_name("pgpaint")
        .ssl_mode(PgSslMode::VerifyFull);
    if let Some(pem) = &target.ssl_root_cert_pem {
        options = options.ssl_root_cert_from_pem(pem.as_bytes().to_vec());
    }
    let connect_options = aurora_dsql_sqlx_connector::DsqlConnectOptionsBuilder::default()
        .pg_connect_options(options)
        .region(Some(aurora_dsql_sqlx_connector::Region::new(
            region.clone(),
        )))
        .build()
        .map_err(map_connector_error)?;
    let pool_options = target_settings(target).max_lifetime(DSQL_MAX_LIFETIME);
    aurora_dsql_sqlx_connector::pool::connect_with(&connect_options, pool_options)
        .await
        .map_err(map_connector_error)
}

fn pg_ssl_mode(mode: SslMode) -> PgSslMode {
    match mode {
        SslMode::Disable => PgSslMode::Disable,
        SslMode::Prefer => PgSslMode::Prefer,
        SslMode::Require => PgSslMode::Require,
        SslMode::VerifyCa => PgSslMode::VerifyCa,
        SslMode::VerifyFull => PgSslMode::VerifyFull,
    }
}

pub(super) async fn cancel_postgres(pool: &PgPool, handle: &CancelHandle) -> Result<bool, DbError> {
    let CancelHandle::BackendPid(pid) = handle else {
        return Ok(false);
    };
    sqlx::query_scalar::<_, bool>("SELECT pg_cancel_backend($1)")
        .bind(pid)
        .fetch_one(pool)
        .await
        .map_err(map_sqlx_error)
}

async fn server_info(pool: &PgPool, target: &ConnectTarget) -> Result<ServerInfo, DbError> {
    let mut connection = pool.acquire().await.map_err(map_connection_error)?;
    let row = sqlx::query("SELECT version(), current_database(), current_user")
        .fetch_one(&mut *connection)
        .await
        .map_err(map_connection_error)?;
    let version: String = row.try_get(0).map_err(map_sqlx_error)?;
    let database: String = row.try_get(1).map_err(map_sqlx_error)?;
    let user: String = row.try_get(2).map_err(map_sqlx_error)?;

    let dsql_function_detected = sqlx::query_scalar::<_, i32>("SELECT sys.dsql_major_version()")
        .fetch_optional(&mut *connection)
        .await
        .ok()
        .flatten()
        .is_some();
    let host = target.host.to_ascii_lowercase();
    let floci_dsql_host = host.contains(".dsql.") && host.ends_with(".localhost.floci.io");
    let detected = Engine::detect(&version, &target.host)
        .or_else(|| (dsql_function_detected || floci_dsql_host).then_some(Engine::AuroraDsql));

    Ok(ServerInfo {
        engine: target.engine,
        version,
        database,
        user,
        engine_mismatch: detected.filter(|engine| *engine != target.engine),
    })
}

async fn format_type_name(entry: &PoolEntry, oid: u32) -> Result<String, DbError> {
    if let Some(name) = entry
        .type_names
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&oid)
        .cloned()
    {
        return Ok(name);
    }
    let sql = format!("SELECT format_type({oid}::oid, NULL)");
    let name = sqlx::query_scalar::<_, String>(AssertSqlSafe(sql))
        .fetch_optional(&entry.pool)
        .await
        .map_err(map_sqlx_error)?
        .unwrap_or_else(|| format!("oid {oid}"));
    entry
        .type_names
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(oid, name.clone());
    Ok(name)
}

async fn column_metadata(entry: &PoolEntry, row: &PgRow) -> Result<Vec<ColumnMeta>, DbError> {
    let mut columns = Vec::with_capacity(row.columns().len());
    for column in row.columns() {
        let type_info = column.type_info();
        let type_oid = type_info.oid().map_or(0, |oid| oid.0);
        let type_name = if type_info.name() == "?" || type_info.name().is_empty() {
            format_type_name(entry, type_oid).await?
        } else {
            type_info.name().to_owned()
        };
        columns.push(ColumnMeta {
            name: column.name().to_owned(),
            type_oid,
            type_name,
        });
    }
    Ok(columns)
}

fn invalid_binary_value(type_name: &str) -> DbError {
    DbError::new(
        DbErrorKind::Query,
        format!("invalid binary representation for PostgreSQL type {type_name}"),
    )
}

fn fixed_binary<const N: usize>(type_name: &str, bytes: &[u8]) -> Result<[u8; N], DbError> {
    bytes
        .try_into()
        .map_err(|_| invalid_binary_value(type_name))
}

fn hex_text(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(HEX[usize::from(byte >> 4)]));
        text.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    text
}

fn numeric_binary_text(bytes: &[u8]) -> Result<String, DbError> {
    if bytes.len() < 8 {
        return Err(invalid_binary_value("NUMERIC"));
    }
    let digit_count = i16::from_be_bytes(fixed_binary("NUMERIC", &bytes[0..2])?);
    let weight = i16::from_be_bytes(fixed_binary("NUMERIC", &bytes[2..4])?);
    let sign = u16::from_be_bytes(fixed_binary("NUMERIC", &bytes[4..6])?);
    let scale = usize::from(u16::from_be_bytes(fixed_binary("NUMERIC", &bytes[6..8])?));
    let digit_count = usize::try_from(digit_count).map_err(|_| invalid_binary_value("NUMERIC"))?;
    let digit_bytes = digit_count
        .checked_mul(2)
        .and_then(|count| count.checked_add(8))
        .ok_or_else(|| invalid_binary_value("NUMERIC"))?;
    if bytes.len() < digit_bytes {
        return Err(invalid_binary_value("NUMERIC"));
    }
    match sign {
        0xC000 => return Ok("NaN".to_owned()),
        0xD000 => return Ok("Infinity".to_owned()),
        0xF000 => return Ok("-Infinity".to_owned()),
        0x0000 | 0x4000 => {}
        _ => return Err(invalid_binary_value("NUMERIC")),
    }
    let digits = bytes[8..digit_bytes]
        .as_chunks::<2>()
        .0
        .iter()
        .copied()
        .map(u16::from_be_bytes)
        .collect::<Vec<_>>();
    let mut text = String::new();
    if sign == 0x4000 {
        text.push('-');
    }
    if weight < 0 {
        text.push('0');
    } else {
        for exponent in (0..=weight).rev() {
            let index =
                usize::try_from(weight - exponent).map_err(|_| invalid_binary_value("NUMERIC"))?;
            let digit = digits.get(index).copied().unwrap_or(0);
            if exponent == weight {
                write!(&mut text, "{digit}").map_err(|_| {
                    DbError::new(DbErrorKind::Internal, "numeric formatting failed")
                })?;
            } else {
                write!(&mut text, "{digit:04}").map_err(|_| {
                    DbError::new(DbErrorKind::Internal, "numeric formatting failed")
                })?;
            }
        }
    }
    if scale > 0 {
        let decimal = text.len();
        text.push('.');
        let fractional_groups = scale.div_ceil(4);
        for group_index in 0..fractional_groups {
            let index = i32::from(weight)
                + 1
                + i32::try_from(group_index).map_err(|_| invalid_binary_value("NUMERIC"))?;
            let digit = usize::try_from(index)
                .ok()
                .and_then(|index| digits.get(index))
                .copied()
                .unwrap_or(0);
            write!(&mut text, "{digit:04}")
                .map_err(|_| DbError::new(DbErrorKind::Internal, "numeric formatting failed"))?;
        }
        text.truncate(decimal + 1 + scale);
    }
    Ok(text)
}

fn timestamp_binary_text(microseconds: i64, with_time_zone: bool) -> Result<String, DbError> {
    const POSTGRES_EPOCH_UNIX_SECONDS: i64 = 946_684_800;
    if microseconds == i64::MAX {
        return Ok("infinity".to_owned());
    }
    if microseconds == i64::MIN {
        return Ok("-infinity".to_owned());
    }
    let seconds = microseconds
        .div_euclid(1_000_000)
        .checked_add(POSTGRES_EPOCH_UNIX_SECONDS)
        .ok_or_else(|| invalid_binary_value("TIMESTAMP"))?;
    let fractional_microseconds = microseconds.rem_euclid(1_000_000);
    let instant = time::OffsetDateTime::from_unix_timestamp(seconds)
        .map_err(|_| invalid_binary_value("TIMESTAMP"))?
        .replace_nanosecond(
            u32::try_from(fractional_microseconds * 1_000)
                .map_err(|_| invalid_binary_value("TIMESTAMP"))?,
        )
        .map_err(|_| invalid_binary_value("TIMESTAMP"))?;
    let mut text = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        instant.year(),
        u8::from(instant.month()),
        instant.day(),
        instant.hour(),
        instant.minute(),
        instant.second(),
    );
    if fractional_microseconds > 0 {
        let mut fraction = format!("{fractional_microseconds:06}");
        while fraction.ends_with('0') {
            fraction.pop();
        }
        text.push('.');
        text.push_str(&fraction);
    }
    if with_time_zone {
        text.push_str("+00");
    }
    Ok(text)
}

fn binary_value_text(type_name: &str, bytes: &[u8]) -> Result<String, DbError> {
    match type_name {
        "BOOL" => match fixed_binary::<1>(type_name, bytes)?[0] {
            0 => Ok("false".to_owned()),
            1 => Ok("true".to_owned()),
            _ => Err(invalid_binary_value(type_name)),
        },
        "INT2" => Ok(i16::from_be_bytes(fixed_binary(type_name, bytes)?).to_string()),
        "INT4" => Ok(i32::from_be_bytes(fixed_binary(type_name, bytes)?).to_string()),
        "INT8" => Ok(i64::from_be_bytes(fixed_binary(type_name, bytes)?).to_string()),
        "OID" | "XID" | "CID" | "REGCLASS" | "REGPROC" | "REGTYPE" | "REGROLE" | "REGNAMESPACE" => {
            Ok(u32::from_be_bytes(fixed_binary(type_name, bytes)?).to_string())
        }
        "FLOAT4" | "FLOAT8" => float_binary_text(type_name, bytes),
        "NUMERIC" => numeric_binary_text(bytes),
        "DATE" => date_binary_text(type_name, bytes),
        "TIME" => time_binary_text(type_name, bytes),
        "TIMESTAMP" => {
            timestamp_binary_text(i64::from_be_bytes(fixed_binary(type_name, bytes)?), false)
        }
        "TIMESTAMPTZ" => {
            timestamp_binary_text(i64::from_be_bytes(fixed_binary(type_name, bytes)?), true)
        }
        _ => other_binary_text(type_name, bytes),
    }
}

fn float_binary_text(type_name: &str, bytes: &[u8]) -> Result<String, DbError> {
    match type_name {
        "FLOAT4" => {
            let value = f32::from_bits(u32::from_be_bytes(fixed_binary(type_name, bytes)?));
            Ok(if value.is_nan() {
                "NaN".to_owned()
            } else if value == f32::INFINITY {
                "Infinity".to_owned()
            } else if value == f32::NEG_INFINITY {
                "-Infinity".to_owned()
            } else {
                value.to_string()
            })
        }
        "FLOAT8" => {
            let value = f64::from_bits(u64::from_be_bytes(fixed_binary(type_name, bytes)?));
            Ok(if value.is_nan() {
                "NaN".to_owned()
            } else if value == f64::INFINITY {
                "Infinity".to_owned()
            } else if value == f64::NEG_INFINITY {
                "-Infinity".to_owned()
            } else {
                value.to_string()
            })
        }
        _ => Err(invalid_binary_value(type_name)),
    }
}

fn date_binary_text(type_name: &str, bytes: &[u8]) -> Result<String, DbError> {
    let days = i32::from_be_bytes(fixed_binary(type_name, bytes)?);
    match days {
        i32::MAX => Ok("infinity".to_owned()),
        i32::MIN => Ok("-infinity".to_owned()),
        _ => {
            let microseconds = i64::from(days)
                .checked_mul(86_400_000_000)
                .ok_or_else(|| invalid_binary_value(type_name))?;
            let date = timestamp_binary_text(microseconds, false)?;
            Ok(date.split(' ').next().unwrap_or(&date).to_owned())
        }
    }
}

fn time_binary_text(type_name: &str, bytes: &[u8]) -> Result<String, DbError> {
    let microseconds = i64::from_be_bytes(fixed_binary(type_name, bytes)?);
    if !(0..=86_400_000_000).contains(&microseconds) {
        return Err(invalid_binary_value(type_name));
    }
    let hours = microseconds / 3_600_000_000;
    let minutes = (microseconds / 60_000_000) % 60;
    let seconds = (microseconds / 1_000_000) % 60;
    let fractional = microseconds % 1_000_000;
    let mut text = format!("{hours:02}:{minutes:02}:{seconds:02}");
    if fractional > 0 {
        let mut fraction = format!("{fractional:06}");
        while fraction.ends_with('0') {
            fraction.pop();
        }
        text.push('.');
        text.push_str(&fraction);
    }
    Ok(text)
}

fn other_binary_text(type_name: &str, bytes: &[u8]) -> Result<String, DbError> {
    match type_name {
        "INTERVAL" => {
            if bytes.len() != 16 {
                return Err(invalid_binary_value(type_name));
            }
            let microseconds = i64::from_be_bytes(fixed_binary(type_name, &bytes[0..8])?);
            let days = i32::from_be_bytes(fixed_binary(type_name, &bytes[8..12])?);
            let months = i32::from_be_bytes(fixed_binary(type_name, &bytes[12..16])?);
            Ok(format!(
                "{months} mons {days} days {microseconds} microseconds"
            ))
        }
        "UUID" => {
            if bytes.len() != 16 {
                return Err(invalid_binary_value(type_name));
            }
            let hex = hex_text(bytes);
            Ok(format!(
                "{}-{}-{}-{}-{}",
                &hex[0..8],
                &hex[8..12],
                &hex[12..16],
                &hex[16..20],
                &hex[20..32],
            ))
        }
        "JSONB" => {
            if bytes.first() != Some(&1) {
                return Err(invalid_binary_value(type_name));
            }
            std::str::from_utf8(&bytes[1..])
                .map(str::to_owned)
                .map_err(|_| invalid_binary_value(type_name))
        }
        "BYTEA" => Ok(format!("\\x{}", hex_text(bytes))),
        _ => {
            if let Ok(text) = std::str::from_utf8(bytes) {
                Ok(text.to_owned())
            } else {
                Ok(format!("\\x{}", hex_text(bytes)))
            }
        }
    }
}

fn row_values(row: &PgRow) -> Result<Vec<Option<String>>, DbError> {
    let mut values = Vec::with_capacity(row.columns().len());
    for index in 0..row.columns().len() {
        let value = row.try_get_raw(index).map_err(map_sqlx_error)?;
        if value.is_null() {
            values.push(None);
        } else if value.format() == PgValueFormat::Text {
            values.push(Some(
                value
                    .as_str()
                    .map_err(|error| DbError::new(DbErrorKind::Query, error.to_string()))?
                    .to_owned(),
            ));
        } else {
            let type_name = row.columns()[index].type_info().name();
            let bytes = value
                .as_bytes()
                .map_err(|error| DbError::new(DbErrorKind::Query, error.to_string()))?;
            values.push(Some(binary_value_text(type_name, bytes)?));
        }
    }
    Ok(values)
}

async fn collect_stream<S>(
    mut stream: S,
    entry: &PoolEntry,
    max_rows: usize,
) -> Result<Vec<StatementResult>, DbError>
where
    S: TryStream<Ok = Either<PgQueryResult, PgRow>, Error = sqlx::Error> + Unpin,
{
    let mut statements = Vec::new();
    let mut current = empty_statement_result();
    while let Some(event) = stream.try_next().await.map_err(map_sqlx_error)? {
        match event {
            Either::Right(row) => {
                if current.columns.is_empty() {
                    current.columns = column_metadata(entry, &row).await?;
                }
                if current.rows.len() < max_rows {
                    current.rows.push(row_values(&row)?);
                } else {
                    current.truncated = true;
                }
            }
            Either::Left(command) => {
                current.rows_affected = command.rows_affected();
                statements.push(std::mem::replace(&mut current, empty_statement_result()));
            }
        }
    }
    if !current.columns.is_empty() || !current.rows.is_empty() || current.truncated {
        statements.push(current);
    }
    Ok(statements)
}

async fn run_execute(
    entry: &PoolEntry,
    mut connection: QueryConnection,
    request: QueryRequest,
) -> Result<Vec<StatementResult>, DbError> {
    let QueryRequest { sql, max_rows, .. } = request;
    if entry.target.engine == Engine::AuroraDsql {
        let mut statements = Vec::new();
        for statement in split_statements(&sql) {
            let stream = raw_sql(AssertSqlSafe(statement.text)).fetch_many(&mut *connection);
            statements.extend(collect_stream(stream, entry, max_rows).await?);
        }
        Ok(statements)
    } else {
        let stream = raw_sql(AssertSqlSafe(sql)).fetch_many(&mut *connection);
        collect_stream(stream, entry, max_rows).await
    }
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn unsupported(message: impl Into<String>) -> DbError {
    DbError::new(DbErrorKind::Unsupported, message)
}

fn column<'a>(detail: &'a TableDetail, name: &str) -> Option<&'a Column> {
    detail.columns.iter().find(|column| column.name == name)
}

fn validate_column<'a>(detail: &'a TableDetail, name: &str) -> Result<&'a Column, DbError> {
    column(detail, name)
        .filter(|column| !column.hidden)
        .ok_or_else(|| unsupported(format!("unknown table column: {name}")))
}

fn quoted_relation(relation: &RelationRef) -> String {
    format!(
        "{}.{}",
        quote_identifier(&relation.schema),
        quote_identifier(&relation.name)
    )
}

fn append_typed_bind(query: &mut QueryBuilder<Postgres>, column: &Column, value: Option<String>) {
    query.push_bind(value);
    query.push("::");
    query.push(column.data_type.as_str());
}

async fn relation_column_oids(
    connection: &mut PoolConnection<Postgres>,
    relation: &RelationRef,
) -> Result<HashMap<String, u32>, DbError> {
    let rows = sqlx::query(
        "SELECT a.attname, a.atttypid::bigint \
         FROM pg_catalog.pg_attribute AS a \
         JOIN pg_catalog.pg_class AS c ON c.oid = a.attrelid \
         JOIN pg_catalog.pg_namespace AS n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relname = $2 \
           AND a.attnum > 0 AND NOT a.attisdropped",
    )
    .bind(&relation.schema)
    .bind(&relation.name)
    .fetch_all(&mut **connection)
    .await
    .map_err(map_sqlx_error)?;
    let mut type_oids = HashMap::with_capacity(rows.len());
    for row in rows {
        let name: String = row.try_get(0).map_err(map_sqlx_error)?;
        let oid: i64 = row.try_get(1).map_err(map_sqlx_error)?;
        let oid = u32::try_from(oid)
            .map_err(|_| DbError::new(DbErrorKind::Internal, "invalid PostgreSQL type OID"))?;
        type_oids.insert(name, oid);
    }
    Ok(type_oids)
}

fn visible_browse_columns<'a>(
    detail: &'a TableDetail,
    request: &BrowseRequest,
) -> Result<Vec<&'a Column>, DbError> {
    let visible_columns = detail
        .columns
        .iter()
        .filter(|column| !column.hidden)
        .collect::<Vec<_>>();
    if visible_columns.is_empty() {
        return Err(unsupported("table has no visible columns"));
    }
    for filter in &request.filters {
        validate_column(detail, &filter.column)?;
        if !matches!(filter.op, FilterOp::IsNull | FilterOp::IsNotNull) && filter.value.is_none() {
            return Err(unsupported(format!(
                "filter value is required for column {}",
                filter.column
            )));
        }
    }
    for sort in &request.sort {
        validate_column(detail, &sort.column)?;
    }
    Ok(visible_columns)
}

fn append_browse_query(
    query: &mut QueryBuilder<Postgres>,
    detail: &TableDetail,
    request: &BrowseRequest,
    visible_columns: &[&Column],
) -> Result<(), DbError> {
    query.push("SELECT ");
    for (index, selected) in visible_columns.iter().enumerate() {
        if index > 0 {
            query.push(", ");
        }
        query
            .push(quote_identifier(&selected.name))
            .push("::text AS ")
            .push(quote_identifier(&selected.name));
    }
    query
        .push(" FROM ")
        .push(quoted_relation(&request.relation));
    if !request.filters.is_empty() {
        query.push(" WHERE ");
        for (index, filter) in request.filters.iter().enumerate() {
            if index > 0 {
                query.push(" AND ");
            }
            let filter_column = validate_column(detail, &filter.column)?;
            query.push(quote_identifier(&filter.column)).push(" ");
            match filter.op {
                FilterOp::Eq => query.push("= "),
                FilterOp::NotEq => query.push("<> "),
                FilterOp::Lt => query.push("< "),
                FilterOp::Lte => query.push("<= "),
                FilterOp::Gt => query.push("> "),
                FilterOp::Gte => query.push(">= "),
                FilterOp::Like => query.push("LIKE "),
                FilterOp::ILike => query.push("ILIKE "),
                FilterOp::IsNull => {
                    query.push("IS NULL");
                    continue;
                }
                FilterOp::IsNotNull => {
                    query.push("IS NOT NULL");
                    continue;
                }
            };
            append_typed_bind(query, filter_column, filter.value.clone());
        }
    }
    if !request.sort.is_empty() {
        query.push(" ORDER BY ");
        for (index, sort) in request.sort.iter().enumerate() {
            if index > 0 {
                query.push(", ");
            }
            query
                .push(quote_identifier(&sort.column))
                .push(if sort.descending { " DESC" } else { " ASC" });
        }
    }
    let offset = i64::try_from(request.offset)
        .map_err(|_| unsupported("browse offset exceeds the database limit"))?;
    query
        .push(" LIMIT ")
        .push_bind(i64::from(request.limit) + 1)
        .push(" OFFSET ")
        .push_bind(offset);
    Ok(())
}

async fn browse_table(
    entry: &PoolEntry,
    mut connection: PoolConnection<Postgres>,
    request: BrowseRequest,
) -> Result<BrowsePage, DbError> {
    let detail = entry
        .dialect
        .table_detail(&mut connection, &request.relation)
        .await?;
    let visible_columns = visible_browse_columns(&detail, &request)?;
    let type_oids = relation_column_oids(&mut connection, &request.relation).await?;
    let metadata = visible_columns
        .iter()
        .map(|column| {
            let type_oid = type_oids.get(&column.name).copied().ok_or_else(|| {
                DbError::new(
                    DbErrorKind::Query,
                    format!(
                        "catalog type information is missing for column {}",
                        column.name
                    ),
                )
            })?;
            Ok(ColumnMeta {
                name: column.name.clone(),
                type_oid,
                type_name: column.data_type.clone(),
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    let limit = usize::try_from(request.limit).unwrap_or(usize::MAX);
    let mut query = QueryBuilder::<Postgres>::new("");
    append_browse_query(&mut query, &detail, &request, &visible_columns)?;
    let rows = query
        .build()
        .fetch_all(&mut *connection)
        .await
        .map_err(map_sqlx_error)?;
    let has_more = rows.len() > limit;
    let mut result_rows = Vec::with_capacity(rows.len().min(limit));
    for row in rows.into_iter().take(limit) {
        result_rows.push(row_values(&row)?);
    }
    Ok(BrowsePage {
        columns: metadata,
        rows: result_rows,
        primary_key: detail.primary_key,
        offset: request.offset,
        limit: request.limit,
        has_more,
        total_estimate: detail.row_estimate,
    })
}

fn validate_values<'a>(
    detail: &'a TableDetail,
    values: &'a std::collections::BTreeMap<String, Option<String>>,
) -> Result<Vec<(&'a Column, &'a Option<String>)>, DbError> {
    values
        .iter()
        .map(|(name, value)| {
            let column = validate_column(detail, name)?;
            Ok((column, value))
        })
        .collect()
}

fn append_insert_query(
    query: &mut QueryBuilder<Postgres>,
    relation: &RelationRef,
    detail: &TableDetail,
    values: &std::collections::BTreeMap<String, Option<String>>,
) -> Result<(), DbError> {
    let validated = validate_values(detail, values)?;
    if validated.is_empty() {
        query
            .push("INSERT INTO ")
            .push(quoted_relation(relation))
            .push(" DEFAULT VALUES");
    } else {
        query
            .push("INSERT INTO ")
            .push(quoted_relation(relation))
            .push(" (");
        for (index, (column, _)) in validated.iter().enumerate() {
            if index > 0 {
                query.push(", ");
            }
            query.push(quote_identifier(&column.name));
        }
        query.push(") VALUES (");
        for (index, (column, value)) in validated.iter().enumerate() {
            if index > 0 {
                query.push(", ");
            }
            append_typed_bind(query, column, (**value).clone());
        }
        query.push(")");
    }
    Ok(())
}

fn append_keyed_edit_query(
    query: &mut QueryBuilder<Postgres>,
    relation: &RelationRef,
    detail: &TableDetail,
    values: Option<&std::collections::BTreeMap<String, Option<String>>>,
    key: Option<&std::collections::BTreeMap<String, Option<String>>>,
) -> Result<(), DbError> {
    if detail.primary_key.is_empty() {
        return Err(unsupported("update and delete require a primary key"));
    }
    let key = key.ok_or_else(|| unsupported("primary key is required"))?;
    if key.len() != detail.primary_key.len()
        || detail
            .primary_key
            .iter()
            .any(|name| !key.contains_key(name))
    {
        return Err(unsupported("the complete primary key is required"));
    }
    if key.values().any(Option::is_none) {
        return Err(unsupported("primary key values cannot be NULL"));
    }
    let validated_key = validate_values(detail, key)?;
    if let Some(values) = values {
        let validated_changes = validate_values(detail, values)?;
        if validated_changes.is_empty() {
            return Err(unsupported("update requires at least one changed column"));
        }
        query
            .push("UPDATE ")
            .push(quoted_relation(relation))
            .push(" SET ");
        for (index, (column, value)) in validated_changes.iter().enumerate() {
            if index > 0 {
                query.push(", ");
            }
            query.push(quote_identifier(&column.name)).push(" = ");
            append_typed_bind(query, column, (**value).clone());
        }
    } else {
        query.push("DELETE FROM ").push(quoted_relation(relation));
    }
    query.push(" WHERE ");
    for (index, (column, value)) in validated_key.iter().enumerate() {
        if index > 0 {
            query.push(" AND ");
        }
        query.push(quote_identifier(&column.name)).push(" = ");
        append_typed_bind(query, column, (**value).clone());
    }
    Ok(())
}

async fn apply_edit(entry: &PoolEntry, edit: RowEdit) -> Result<u64, DbError> {
    if entry.target.read_only {
        return Err(unsupported(
            "writes are disabled for this read-only connection",
        ));
    }
    let (relation, values, key, is_insert) = match edit {
        RowEdit::Insert { relation, values } => (relation, Some(values), None, true),
        RowEdit::Update {
            relation,
            key,
            changes,
        } => (relation, Some(changes), Some(key), false),
        RowEdit::Delete { relation, key } => (relation, None, Some(key), false),
    };
    let mut connection = entry.pool.acquire().await.map_err(map_sqlx_error)?;
    let detail = entry
        .dialect
        .table_detail(&mut connection, &relation)
        .await?;
    let mut query = QueryBuilder::<Postgres>::new("");
    if is_insert {
        let values = values
            .as_ref()
            .ok_or_else(|| unsupported("insert values are missing"))?;
        append_insert_query(&mut query, &relation, &detail, values)?;
    } else {
        append_keyed_edit_query(
            &mut query,
            &relation,
            &detail,
            values.as_ref(),
            key.as_ref(),
        )?;
    }
    drop(connection);
    let mut transaction = entry.pool.begin().await.map_err(map_sqlx_error)?;
    let result = match query.build().execute(&mut *transaction).await {
        Ok(result) => result.rows_affected(),
        Err(error) => {
            let _ = transaction.rollback().await;
            return Err(map_sqlx_error(error));
        }
    };
    if !is_insert && result != 1 {
        let _ = transaction.rollback().await;
        return Err(DbError::new(
            DbErrorKind::Query,
            format!("row edit expected exactly one affected row, got {result}"),
        ));
    }
    transaction.commit().await.map_err(map_sqlx_error)?;
    Ok(result)
}

#[async_trait]
impl DatabaseGateway for SqlxGateway {
    async fn test(&self, target: &ConnectTarget) -> Result<ServerInfo, DbError> {
        let pool = dialect(target.engine).connect(target).await?;
        let info = server_info(&pool, target).await;
        pool.close().await;
        info
    }

    async fn connect(
        &self,
        key: &ConnectionKey,
        target: ConnectTarget,
    ) -> Result<ServerInfo, DbError> {
        self.ensure_eviction_task();
        let dialect = dialect(target.engine);
        let pool = dialect.connect(&target).await?;
        let server_info = match server_info(&pool, &target).await {
            Ok(info) => info,
            Err(error) => {
                pool.close().await;
                return Err(error);
            }
        };
        let entry = Arc::new(PoolEntry {
            pool,
            dialect,
            target,
            server_info: server_info.clone(),
            last_used: StdMutex::new(Instant::now()),
            type_names: RwLock::new(HashMap::new()),
        });
        let previous = self
            .inner
            .pools
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.clone(), entry);
        if let Some(previous) = previous {
            previous.pool.close().await;
        }
        Ok(server_info)
    }

    async fn disconnect(&self, key: &ConnectionKey) {
        let active = {
            let mut running = self
                .inner
                .running
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let keys = running
                .keys()
                .filter(|(running_key, _)| running_key == key)
                .cloned()
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|running_key| running.remove(&running_key))
                .collect::<Vec<_>>()
        };
        for query in active {
            query.close_connection.store(true, Ordering::Release);
            query.abort.abort();
        }
        let entry = self
            .inner
            .pools
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(key);
        if let Some(entry) = entry {
            entry.pool.close().await;
        }
    }

    fn server_info(&self, key: &ConnectionKey) -> Option<ServerInfo> {
        self.inner
            .pools
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .map(|entry| entry.server_info.clone())
    }

    async fn schema_tree(&self, key: &ConnectionKey) -> Result<SchemaTree, DbError> {
        let entry = self.entry(key)?;
        let mut connection = entry.pool.acquire().await.map_err(map_sqlx_error)?;
        entry.dialect.schema_tree(&mut connection).await
    }

    async fn table_detail(
        &self,
        key: &ConnectionKey,
        relation: &RelationRef,
    ) -> Result<TableDetail, DbError> {
        let entry = self.entry(key)?;
        let mut connection = entry.pool.acquire().await.map_err(map_sqlx_error)?;
        entry.dialect.table_detail(&mut connection, relation).await
    }

    async fn relationships(
        &self,
        key: &ConnectionKey,
        schema: &str,
    ) -> Result<Vec<Relationship>, DbError> {
        let entry = self.entry(key)?;
        let mut connection = entry.pool.acquire().await.map_err(map_sqlx_error)?;
        entry.dialect.relationships(&mut connection, schema).await
    }

    async fn completion_schema(&self, key: &ConnectionKey) -> Result<CompletionSchema, DbError> {
        let entry = self.entry(key)?;
        let mut connection = entry.pool.acquire().await.map_err(map_sqlx_error)?;
        entry.dialect.completion_schema(&mut connection).await
    }

    async fn execute(
        &self,
        key: &ConnectionKey,
        request: QueryRequest,
    ) -> Result<QueryResult, DbError> {
        let entry = self.entry(key)?;
        self.execute_request(key, entry, request).await
    }

    async fn cancel(&self, key: &ConnectionKey, id: QueryId) -> Result<bool, DbError> {
        self.cancel_active(key, id).await
    }

    async fn stream(
        &self,
        key: &ConnectionKey,
        sql: String,
    ) -> Result<BoxStream<'static, Result<RowEvent, DbError>>, DbError> {
        let entry = self.entry(key)?;
        let statements = split_statements(&sql);
        if statements.len() != 1 {
            return Err(unsupported("stream requires exactly one SQL statement"));
        }
        let statement = statements[0].text.to_owned();
        if !matches!(
            crate::domain::database::classify(&statement),
            StatementClass::Query
        ) {
            return Err(unsupported("stream requires a query statement"));
        }
        let stream = async_stream::try_stream! {
            let pooled = entry.pool.acquire().await.map_err(map_sqlx_error)?;
            let mut connection = ExportConnection {
                connection: pooled,
                completed: false,
            };
            let mut sent_columns = false;
            if entry.target.engine.capabilities().server_side_cursors {
                raw_sql("BEGIN READ ONLY")
                    .execute(&mut *connection)
                    .await
                    .map_err(map_sqlx_error)?;
                let declare = format!("DECLARE pgpaint_export_cursor NO SCROLL CURSOR FOR {statement}");
                if let Err(error) = raw_sql(AssertSqlSafe(declare)).execute(&mut *connection).await {
                    let _ = raw_sql("ROLLBACK").execute(&mut *connection).await;
                    Err::<(), DbError>(map_sqlx_error(error))?;
                }
                loop {
                    let mut fetched = 0usize;
                    let mut rows = raw_sql("FETCH FORWARD 256 FROM pgpaint_export_cursor")
                        .fetch_many(&mut *connection);
                    while let Some(event) = rows.try_next().await.map_err(map_sqlx_error)? {
                        if let Either::Right(row) = event {
                            if !sent_columns {
                                yield RowEvent::Columns(column_metadata(&entry, &row).await?);
                                sent_columns = true;
                            }
                            yield RowEvent::Row(row_values(&row)?);
                            fetched += 1;
                        }
                    }
                    if fetched == 0 {
                        break;
                    }
                }
                raw_sql("CLOSE pgpaint_export_cursor")
                    .execute(&mut *connection)
                    .await
                    .map_err(map_sqlx_error)?;
                raw_sql("COMMIT")
                    .execute(&mut *connection)
                    .await
                    .map_err(map_sqlx_error)?;
            } else {
                let mut rows = raw_sql(AssertSqlSafe(statement)).fetch_many(&mut *connection);
                while let Some(event) = rows.try_next().await.map_err(map_sqlx_error)? {
                    if let Either::Right(row) = event {
                        if !sent_columns {
                            yield RowEvent::Columns(column_metadata(&entry, &row).await?);
                            sent_columns = true;
                        }
                        yield RowEvent::Row(row_values(&row)?);
                    }
                }
            }
            if !sent_columns {
                yield RowEvent::Columns(Vec::new());
            }
            connection.completed = true;
        };
        Ok(Box::pin(stream))
    }

    async fn explain(
        &self,
        key: &ConnectionKey,
        sql: &str,
        analyze: bool,
    ) -> Result<ExplainResult, DbError> {
        let statements = split_statements(sql);
        if statements.len() != 1 {
            return Err(unsupported("EXPLAIN requires exactly one SQL statement"));
        }
        let entry = self.entry(key)?;
        let explain_sql = entry.dialect.explain_sql(statements[0].text, analyze);
        let result = self
            .execute_request(
                key,
                Arc::clone(&entry),
                QueryRequest {
                    id: QueryId(uuid::Uuid::new_v4()),
                    sql: explain_sql,
                    max_rows: 10_000,
                    timeout: None,
                },
            )
            .await?;
        let output = result
            .statements
            .into_iter()
            .next()
            .unwrap_or_else(empty_statement_result);
        Ok(entry.dialect.parse_plan(&output, analyze))
    }

    async fn browse(
        &self,
        key: &ConnectionKey,
        request: BrowseRequest,
    ) -> Result<BrowsePage, DbError> {
        let entry = self.entry(key)?;
        let connection = entry.pool.acquire().await.map_err(map_sqlx_error)?;
        browse_table(&entry, connection, request).await
    }

    async fn apply_edit(&self, key: &ConnectionKey, edit: RowEdit) -> Result<u64, DbError> {
        let entry = self.entry(key)?;
        apply_edit(&entry, edit).await
    }

    #[allow(deprecated)]
    async fn engine_view(
        &self,
        key: &ConnectionKey,
        view_id: &str,
        relation: Option<&RelationRef>,
    ) -> Result<StatementResult, DbError> {
        let entry = self.entry(key)?;
        let view = entry
            .target
            .engine
            .views()
            .iter()
            .find(|view| view.id == view_id)
            .ok_or_else(|| unsupported(format!("unknown engine view: {view_id}")))?;
        match (view.scope, relation) {
            (ViewScope::Database, None) | (ViewScope::Relation, Some(_)) => {}
            (ViewScope::Database, Some(_)) => {
                return Err(unsupported(
                    "database-scoped view does not accept a relation",
                ));
            }
            (ViewScope::Relation, None) => {
                return Err(unsupported("relation-scoped view requires a relation"));
            }
        }
        let sql = entry
            .dialect
            .view_sql(view_id)
            .ok_or_else(|| unsupported(format!("engine view is not implemented: {view_id}")))?;
        let mut connection = entry.pool.acquire().await.map_err(map_sqlx_error)?;
        let query = sqlx::query(sql);
        let query = if let Some(relation) = relation {
            query
                .bind(relation.schema.clone())
                .bind(relation.name.clone())
        } else {
            query
        };
        let stream = query.fetch_many(&mut *connection);
        let statements = collect_stream(stream, &entry, MAX_ENGINE_VIEW_ROWS).await?;
        Ok(statements
            .into_iter()
            .next()
            .unwrap_or_else(empty_statement_result))
    }
}
