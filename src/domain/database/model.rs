use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::engine::Engine;

/// Facts read from the server right after connecting.
#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    pub engine: Engine,
    pub version: String,
    pub database: String,
    pub user: String,
    /// The engine detected from the server differs from the profile's engine.
    pub engine_mismatch: Option<Engine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RelationRef {
    pub schema: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Table,
    PartitionedTable,
    View,
    MaterializedView,
    ForeignTable,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaTree {
    pub schemas: Vec<SchemaNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaNode {
    pub name: String,
    pub relations: Vec<RelationSummary>,
    pub sequences: Vec<String>,
    pub routines: Vec<RoutineSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelationSummary {
    pub name: String,
    pub kind: RelationKind,
    pub row_estimate: Option<i64>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineKind {
    Function,
    Procedure,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutineSummary {
    pub name: String,
    pub kind: RoutineKind,
    /// Argument list as printed by `pg_get_function_identity_arguments`.
    pub arguments: String,
    pub return_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableDetail {
    pub relation: RelationRef,
    pub kind: RelationKind,
    pub columns: Vec<Column>,
    /// Ordered primary-key column names; empty when the relation has none.
    pub primary_key: Vec<String>,
    pub indexes: Vec<Index>,
    pub constraints: Vec<Constraint>,
    pub foreign_keys: Vec<ForeignKey>,
    pub row_estimate: Option<i64>,
    pub comment: Option<String>,
    /// Reconstructed `CREATE ...` statement(s) for display.
    pub ddl: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Column {
    pub name: String,
    pub ordinal: i32,
    /// `format_type(atttypid, atttypmod)` output.
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    /// `ALWAYS`/`BY DEFAULT` identity or generated-column expression, if any.
    pub generated: Option<String>,
    pub comment: Option<String>,
    /// Engine-managed column hidden from `SELECT *` (`CockroachDB` `rowid`).
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Index {
    pub name: String,
    pub definition: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub primary: bool,
    /// Build state for engines with asynchronous index creation (DSQL).
    pub status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    PrimaryKey,
    Unique,
    Check,
    ForeignKey,
    Exclusion,
}

#[derive(Debug, Clone, Serialize)]
pub struct Constraint {
    pub name: String,
    pub kind: ConstraintKind,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced: RelationRef,
    pub referenced_columns: Vec<String>,
    pub on_update: String,
    pub on_delete: String,
}

/// Edge of the ER diagram.
#[derive(Debug, Clone, Serialize)]
pub struct Relationship {
    pub name: Option<String>,
    pub from: RelationRef,
    pub from_columns: Vec<String>,
    pub to: RelationRef,
    pub to_columns: Vec<String>,
    /// Derived from naming conventions because the engine has no foreign keys.
    pub inferred: bool,
}

/// Payload for the SQL editor's autocompletion (`schema -> table -> columns`).
#[derive(Debug, Clone, Serialize)]
pub struct CompletionSchema {
    pub default_schema: String,
    pub schemas: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}
