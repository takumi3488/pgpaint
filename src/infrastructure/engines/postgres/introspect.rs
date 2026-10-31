//! Catalog introspection for PostgreSQL.
use sqlx::PgConnection;

use crate::domain::database::{
    CompletionSchema, DbError, RelationRef, Relationship, SchemaTree, TableDetail,
};

pub(super) async fn schema_tree(conn: &mut PgConnection) -> Result<SchemaTree, DbError> {
    super::super::catalog::schema_tree(conn, crate::domain::database::Engine::Postgres).await
}

pub(super) async fn table_detail(
    conn: &mut PgConnection,
    relation: &RelationRef,
) -> Result<TableDetail, DbError> {
    super::super::catalog::table_detail(conn, relation, crate::domain::database::Engine::Postgres)
        .await
}

pub(super) async fn relationships(
    conn: &mut PgConnection,
    schema: &str,
) -> Result<Vec<Relationship>, DbError> {
    super::super::catalog::relationships(conn, schema, crate::domain::database::Engine::Postgres)
        .await
}

pub(super) async fn completion_schema(
    conn: &mut PgConnection,
) -> Result<CompletionSchema, DbError> {
    super::super::catalog::completion_schema(conn).await
}
