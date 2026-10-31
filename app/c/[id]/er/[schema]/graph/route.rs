use std::collections::HashSet;

use axum::{extract::{Path, Query}, Json};
use futures::{StreamExt, TryStreamExt};
use gluon::prelude::*;
use pgpaint::domain::database::{
    DatabaseGateway, DbError, RelationKind, RelationRef, Relationship,
};
use pgpaint::web::Workspace;
use serde::{Deserialize, Serialize};

const MAX_TABLES: usize = 150;
const DETAIL_CONCURRENCY: usize = 8;

#[derive(Deserialize)]
pub struct GraphQuery {
    filter: Option<String>,
}

#[derive(Serialize)]
pub struct GraphResponse {
    tables: Vec<GraphTable>,
    edges: Vec<GraphEdge>,
    total_tables: usize,
}

#[derive(Serialize)]
pub struct GraphTable {
    name: String,
    columns: Vec<GraphColumn>,
}

#[derive(Serialize)]
pub struct GraphColumn {
    name: String,
    data_type: String,
    pk: bool,
    fk: bool,
    nullable: bool,
}

#[derive(Serialize)]
pub struct GraphEdge {
    from_table: String,
    from_columns: Vec<String>,
    to_table: String,
    to_columns: Vec<String>,
    inferred: bool,
    cardinality: &'static str,
}

pub async fn get(
    Path((_id, schema)): Path<(String, String)>,
    workspace: Workspace,
    Inject(gateway): Inject<dyn DatabaseGateway>,
    Query(query): Query<GraphQuery>,
) -> gluon::Result<Json<GraphResponse>> {
    if !workspace.is_connected() {
        return Err(gluon::AppError::Conflict(
            "Connect to the database to load its ER diagram.".to_owned(),
        ));
    }

    let filter = query
        .filter
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let lowered_filter = filter.as_deref().map(str::to_lowercase);
    let tree = gateway.schema_tree(&workspace.key).await?;
    let schema_node = tree
        .schemas
        .into_iter()
        .find(|item| item.name == schema)
        .ok_or(gluon::AppError::NotFound)?;

    let mut table_names = schema_node
        .relations
        .into_iter()
        .filter(|relation| {
            matches!(
                relation.kind,
                RelationKind::Table | RelationKind::PartitionedTable | RelationKind::ForeignTable
            )
        })
        .map(|relation| relation.name)
        .filter(|name| {
            lowered_filter
                .as_ref()
                .is_none_or(|filter| name.to_lowercase().contains(filter))
        })
        .collect::<Vec<_>>();
    table_names.sort_unstable();
    let total_tables = table_names.len();
    table_names.truncate(MAX_TABLES);

    let gateway = &*gateway;
    let connection_key = &workspace.key;
    let details = futures::stream::iter(table_names.into_iter().map(|name| {
        let relation = RelationRef {
            schema: schema.clone(),
            name: name.clone(),
        };
        async move {
            let detail = gateway.table_detail(connection_key, &relation).await?;
            Ok::<_, DbError>((name, detail))
        }
    }))
    .buffer_unordered(DETAIL_CONCURRENCY)
    .try_collect::<Vec<_>>()
    .await?;

    let table_names = details
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<HashSet<_>>();
    let edges = gateway
        .relationships(&workspace.key, &schema)
        .await?
        .into_iter()
        .filter(|edge| {
            table_names.contains(edge.from.name.as_str())
                && table_names.contains(edge.to.name.as_str())
        })
        .map(graph_edge)
        .collect();

    let mut tables = details
        .into_iter()
        .map(|(name, detail)| {
            let primary_key = detail.primary_key.iter().collect::<HashSet<_>>();
            let foreign_key_columns = detail
                .foreign_keys
                .iter()
                .flat_map(|foreign_key| &foreign_key.columns)
                .collect::<HashSet<_>>();
            GraphTable {
                name,
                columns: detail
                    .columns
                    .into_iter()
                    .map(|column| GraphColumn {
                        pk: primary_key.contains(&column.name),
                        fk: foreign_key_columns.contains(&column.name),
                        name: column.name,
                        data_type: column.data_type,
                        nullable: column.nullable,
                    })
                    .collect(),
            }
        })
        .collect::<Vec<_>>();
    tables.sort_unstable_by(|left, right| left.name.cmp(&right.name));

    Ok(Json(GraphResponse {
        tables,
        edges,
        total_tables,
    }))
}

fn graph_edge(edge: Relationship) -> GraphEdge {
    GraphEdge {
        from_table: edge.from.name,
        from_columns: edge.from_columns,
        to_table: edge.to.name,
        to_columns: edge.to_columns,
        inferred: edge.inferred,
        cardinality: "N:1",
    }
}
