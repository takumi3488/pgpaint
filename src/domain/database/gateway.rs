use async_trait::async_trait;
use futures::stream::BoxStream;

use super::engine::Engine;
use super::error::DbError;
use super::model::{
    CompletionSchema, RelationRef, Relationship, SchemaTree, ServerInfo, TableDetail,
};
use super::query::{
    BrowsePage, BrowseRequest, ExplainResult, QueryId, QueryRequest, QueryResult, RowEdit,
    RowEvent, StatementResult,
};
use crate::domain::connection_profile::ConnectionProfileId;

/// Who holds a live connection: the browser session (local mode) or the
/// authenticated user (shared mode).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OwnerId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionKey {
    pub owner: OwnerId,
    pub profile: ConnectionProfileId,
}

/// Plaintext credential; never printed.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[derive(Debug, Clone)]
pub enum ConnectAuth {
    Password(Secret),
    /// IAM auth token generated per connection (`admin` selects
    /// `DbConnectAdmin`). Credentials come from the AWS default chain.
    DsqlIam {
        region: String,
        admin: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SslMode {
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

/// Everything needed to open connections for one profile.
#[derive(Debug, Clone)]
pub struct ConnectTarget {
    pub engine: Engine,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub auth: ConnectAuth,
    pub ssl_mode: SslMode,
    /// PEM bundle trusted in addition to the system roots (e.g. local DSQL emulator).
    pub ssl_root_cert_pem: Option<String>,
    /// Every session starts with `default_transaction_read_only = on`.
    pub read_only: bool,
    pub statement_timeout_ms: Option<u64>,
}

/// Port to the user's target databases. Implementations dispatch on
/// `ConnectTarget::engine` and keep one pool per `ConnectionKey`.
#[async_trait]
pub trait DatabaseGateway: Send + Sync {
    /// Opens a connection without registering it (connection-form "Test").
    async fn test(&self, target: &ConnectTarget) -> Result<ServerInfo, DbError>;
    /// Opens (or replaces) the pool for `key`.
    async fn connect(
        &self,
        key: &ConnectionKey,
        target: ConnectTarget,
    ) -> Result<ServerInfo, DbError>;
    async fn disconnect(&self, key: &ConnectionKey);
    /// Server facts of an open connection; `None` when not connected.
    fn server_info(&self, key: &ConnectionKey) -> Option<ServerInfo>;

    async fn schema_tree(&self, key: &ConnectionKey) -> Result<SchemaTree, DbError>;
    async fn table_detail(
        &self,
        key: &ConnectionKey,
        relation: &RelationRef,
    ) -> Result<TableDetail, DbError>;
    /// Foreign keys of `schema`, or inferred edges when the engine has none.
    async fn relationships(
        &self,
        key: &ConnectionKey,
        schema: &str,
    ) -> Result<Vec<Relationship>, DbError>;
    async fn completion_schema(&self, key: &ConnectionKey) -> Result<CompletionSchema, DbError>;

    /// Runs one or more statements; returns when all finished or one failed.
    async fn execute(
        &self,
        key: &ConnectionKey,
        request: QueryRequest,
    ) -> Result<QueryResult, DbError>;
    /// Interrupts the run started with `id`; `false` when it already ended.
    async fn cancel(&self, key: &ConnectionKey, id: QueryId) -> Result<bool, DbError>;
    /// Streams every row of a single statement (export).
    async fn stream(
        &self,
        key: &ConnectionKey,
        sql: String,
    ) -> Result<BoxStream<'static, Result<RowEvent, DbError>>, DbError>;

    /// `analyze = true` executes the statement; callers must guard writes.
    async fn explain(
        &self,
        key: &ConnectionKey,
        sql: &str,
        analyze: bool,
    ) -> Result<ExplainResult, DbError>;

    async fn browse(
        &self,
        key: &ConnectionKey,
        request: BrowseRequest,
    ) -> Result<BrowsePage, DbError>;
    /// Applies a single-row edit; returns affected rows (must be 1 for update/delete).
    async fn apply_edit(&self, key: &ConnectionKey, edit: RowEdit) -> Result<u64, DbError>;

    /// Runs one of `Engine::views()` by id; `relation` is required for
    /// `ViewScope::Relation` views.
    async fn engine_view(
        &self,
        key: &ConnectionKey,
        view_id: &str,
        relation: Option<&RelationRef>,
    ) -> Result<StatementResult, DbError>;
}
