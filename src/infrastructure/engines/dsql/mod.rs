mod explain;
mod introspect;
mod session;
mod views;

use async_trait::async_trait;
use sqlx::{PgConnection, PgPool};

use super::{CancelHandle, Dialect};
use crate::domain::database::{
    CompletionSchema, ConnectTarget, DbError, Engine, ExplainResult, RelationRef, Relationship,
    SchemaTree, StatementResult, TableDetail,
};

pub struct Dsql;

#[async_trait]
impl Dialect for Dsql {
    fn engine(&self) -> Engine {
        Engine::AuroraDsql
    }

    async fn connect(&self, target: &ConnectTarget) -> Result<PgPool, DbError> {
        session::connect(target).await
    }

    async fn cancel_handle(&self, conn: &mut PgConnection) -> Result<CancelHandle, DbError> {
        Ok(session::cancel_handle(conn))
    }

    async fn cancel(&self, pool: &PgPool, handle: &CancelHandle) -> Result<bool, DbError> {
        Ok(session::cancel(pool, handle))
    }

    async fn schema_tree(&self, conn: &mut PgConnection) -> Result<SchemaTree, DbError> {
        introspect::schema_tree(conn).await
    }

    async fn table_detail(
        &self,
        conn: &mut PgConnection,
        relation: &RelationRef,
    ) -> Result<TableDetail, DbError> {
        introspect::table_detail(conn, relation).await
    }

    async fn relationships(
        &self,
        conn: &mut PgConnection,
        schema: &str,
    ) -> Result<Vec<Relationship>, DbError> {
        introspect::relationships(conn, schema).await
    }

    async fn completion_schema(
        &self,
        conn: &mut PgConnection,
    ) -> Result<CompletionSchema, DbError> {
        introspect::completion_schema(conn).await
    }

    fn explain_sql(&self, sql: &str, analyze: bool) -> String {
        explain::explain_sql(sql, analyze)
    }

    fn parse_plan(&self, output: &StatementResult, analyzed: bool) -> ExplainResult {
        explain::parse_plan(output, analyzed)
    }

    fn view_sql(&self, view_id: &str) -> Option<&'static str> {
        views::view_sql(view_id)
    }
}
