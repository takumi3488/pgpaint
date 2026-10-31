use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::model::RelationRef;

/// Client-generated id so a sibling request can cancel or page a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QueryId(pub uuid::Uuid);

#[derive(Debug, Clone)]
pub struct QueryRequest {
    pub id: QueryId,
    pub sql: String,
    /// Rows kept per statement; the rest are discarded and `truncated` is set.
    pub max_rows: usize,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnMeta {
    pub name: String,
    pub type_oid: u32,
    pub type_name: String,
}

/// Text-format value as sent by the server; `None` is SQL NULL.
pub type Cell = Option<String>;

#[derive(Debug, Clone, Serialize)]
pub struct StatementResult {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Cell>>,
    pub rows_affected: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryResult {
    pub statements: Vec<StatementResult>,
    pub elapsed_ms: u64,
}

/// Item of a streamed result (export): the header precedes its rows.
#[derive(Debug, Clone)]
pub enum RowEvent {
    Columns(Vec<ColumnMeta>),
    Row(Vec<Cell>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    Like,
    ILike,
    IsNull,
    IsNotNull,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub column: String,
    pub op: FilterOp,
    /// Ignored by `IsNull`/`IsNotNull`.
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sort {
    pub column: String,
    pub descending: bool,
}

#[derive(Debug, Clone)]
pub struct BrowseRequest {
    pub relation: RelationRef,
    pub filters: Vec<Filter>,
    pub sort: Vec<Sort>,
    pub offset: u64,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowsePage {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Cell>>,
    pub primary_key: Vec<String>,
    pub offset: u64,
    pub limit: u32,
    pub has_more: bool,
    pub total_estimate: Option<i64>,
}

/// Column name -> text value (`None` = NULL). Values are bound as text and
/// cast to the column's catalog type by the gateway.
pub type RowValues = BTreeMap<String, Cell>;

#[derive(Debug, Clone)]
pub enum RowEdit {
    Insert {
        relation: RelationRef,
        values: RowValues,
    },
    Update {
        relation: RelationRef,
        key: RowValues,
        changes: RowValues,
    },
    Delete {
        relation: RelationRef,
        key: RowValues,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplainResult {
    /// Plan exactly as returned by the server.
    pub raw: String,
    /// Normalized tree; `None` when the engine's output could not be parsed.
    pub plan: Option<PlanNode>,
    pub analyzed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanNode {
    pub node_type: String,
    pub relation: Option<String>,
    pub estimated_rows: Option<f64>,
    pub actual_rows: Option<f64>,
    pub total_cost: Option<f64>,
    pub actual_time_ms: Option<f64>,
    pub loops: Option<u64>,
    /// Remaining engine-specific attributes, in server order.
    pub details: Vec<(String, String)>,
    pub children: Vec<PlanNode>,
}
