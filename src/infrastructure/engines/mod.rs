//! Engine-specific behavior behind `DatabaseGateway`.
//!
//! Ownership (parallel slices): `gateway.rs` + `*/session.rs` = gateway slice;
//! `catalog.rs` + `*/introspect.rs` = introspection slice;
//! `*/explain.rs` + `*/views.rs` = explain/views slice.

pub mod catalog;
pub mod cockroach;
pub mod dsql;
pub mod gateway;
mod plan;
pub mod postgres;
pub mod yugabyte;

use async_trait::async_trait;
use sqlx::{PgConnection, PgPool};

use crate::domain::database::{
    CompletionSchema, ConnectTarget, DbError, Engine, ExplainResult, RelationRef, Relationship,
    SchemaTree, StatementResult, TableDetail,
};

/// What `Dialect::cancel` needs, captured on the connection that runs a statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelHandle {
    /// `pg_backend_pid()` of the running connection.
    BackendPid(i32),
    /// `CockroachDB` `SHOW session_id`; the running query id is looked up at cancel time.
    CockroachSession(String),
    /// No server-side cancel: the gateway aborts the task and drops the connection.
    CloseConnection,
}

/// sqlx-aware engine implementation; one stateless value per `Engine`.
#[async_trait]
pub trait Dialect: Send + Sync {
    fn engine(&self) -> Engine;

    /// Opens a pool whose connections already applied the target's session
    /// settings (read-only, statement timeout).
    async fn connect(&self, target: &ConnectTarget) -> Result<PgPool, DbError>;
    async fn cancel_handle(&self, conn: &mut PgConnection) -> Result<CancelHandle, DbError>;
    /// `Ok(false)` when nothing was running.
    async fn cancel(&self, pool: &PgPool, handle: &CancelHandle) -> Result<bool, DbError>;

    async fn schema_tree(&self, conn: &mut PgConnection) -> Result<SchemaTree, DbError>;
    async fn table_detail(
        &self,
        conn: &mut PgConnection,
        relation: &RelationRef,
    ) -> Result<TableDetail, DbError>;
    async fn relationships(
        &self,
        conn: &mut PgConnection,
        schema: &str,
    ) -> Result<Vec<Relationship>, DbError>;
    async fn completion_schema(&self, conn: &mut PgConnection)
    -> Result<CompletionSchema, DbError>;

    /// Wraps a single statement in this engine's EXPLAIN syntax.
    fn explain_sql(&self, sql: &str, analyze: bool) -> String;
    fn parse_plan(&self, output: &StatementResult, analyzed: bool) -> ExplainResult;

    /// SQL for one of `Engine::views()`; `$1`/`$2` bind schema/relation name
    /// for `ViewScope::Relation` views.
    fn view_sql(&self, view_id: &str) -> Option<&'static str>;
}

#[must_use]
pub fn dialect(engine: Engine) -> &'static dyn Dialect {
    match engine {
        Engine::Postgres => &postgres::Postgres,
        Engine::CockroachDb => &cockroach::Cockroach,
        Engine::YugabyteDb => &yugabyte::Yugabyte,
        Engine::AuroraDsql => &dsql::Dsql,
    }
}
