use serde::{Deserialize, Serialize};

/// PostgreSQL-compatible server families pgpaint supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    Postgres,
    AuroraDsql,
    CockroachDb,
    YugabyteDb,
}

impl Engine {
    pub const ALL: [Engine; 4] = [
        Engine::Postgres,
        Engine::AuroraDsql,
        Engine::CockroachDb,
        Engine::YugabyteDb,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Engine::Postgres => "PostgreSQL",
            Engine::AuroraDsql => "Aurora DSQL",
            Engine::CockroachDb => "CockroachDB",
            Engine::YugabyteDb => "YugabyteDB",
        }
    }

    #[must_use]
    pub fn default_port(self) -> u16 {
        match self {
            Engine::Postgres | Engine::AuroraDsql => 5432,
            Engine::CockroachDb => 26257,
            Engine::YugabyteDb => 5433,
        }
    }

    /// Detects the engine from `SELECT version()` output and the connection host.
    /// Returns `None` when the server looks like plain PostgreSQL; callers then
    /// trust the engine the user selected (DSQL emulators report plain PostgreSQL).
    #[must_use]
    pub fn detect(version: &str, host: &str) -> Option<Engine> {
        let host = host.to_ascii_lowercase();
        if host.contains(".dsql") && host.ends_with(".on.aws") {
            Some(Engine::AuroraDsql)
        } else if version.contains("CockroachDB") {
            Some(Engine::CockroachDb)
        } else if version.contains("-YB-") {
            Some(Engine::YugabyteDb)
        } else {
            None
        }
    }

    #[must_use]
    pub fn capabilities(self) -> &'static Capabilities {
        match self {
            Engine::Postgres => &POSTGRES,
            Engine::AuroraDsql => &AURORA_DSQL,
            Engine::CockroachDb => &COCKROACH_DB,
            Engine::YugabyteDb => &YUGABYTE_DB,
        }
    }

    /// Engine-specific diagnostic views. The SQL behind each id lives in the
    /// engine's infrastructure dialect.
    #[must_use]
    pub fn views(self) -> &'static [EngineView] {
        match self {
            Engine::Postgres => POSTGRES_VIEWS,
            Engine::AuroraDsql => AURORA_DSQL_VIEWS,
            Engine::CockroachDb => COCKROACH_DB_VIEWS,
            Engine::YugabyteDb => YUGABYTE_DB_VIEWS,
        }
    }
}

const POSTGRES_VIEWS: &[EngineView] = &[
    EngineView::new("activity", "Activity", ViewScope::Database),
    EngineView::new("locks", "Locks", ViewScope::Database),
    EngineView::new("table_stats", "Table statistics", ViewScope::Relation),
];
const AURORA_DSQL_VIEWS: &[EngineView] = &[EngineView::new(
    "jobs",
    "Async index jobs",
    ViewScope::Database,
)];
const COCKROACH_DB_VIEWS: &[EngineView] = &[
    EngineView::new("regions", "Regions", ViewScope::Database),
    EngineView::new("jobs", "Jobs", ViewScope::Database),
    EngineView::new("sessions", "Sessions", ViewScope::Database),
    EngineView::new("ranges", "Ranges", ViewScope::Relation),
];
const YUGABYTE_DB_VIEWS: &[EngineView] = &[
    EngineView::new("servers", "Servers", ViewScope::Database),
    EngineView::new("activity", "Activity", ViewScope::Database),
    EngineView::new("tablets", "Tablets", ViewScope::Relation),
    EngineView::new("table_properties", "Table properties", ViewScope::Relation),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewScope {
    Database,
    Relation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EngineView {
    pub id: &'static str,
    pub title: &'static str,
    pub scope: ViewScope,
}

impl EngineView {
    const fn new(id: &'static str, title: &'static str, scope: ViewScope) -> Self {
        Self { id, title, scope }
    }
}

/// How a running statement is interrupted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelStrategy {
    /// `SELECT pg_cancel_backend(pid)` from a sibling connection.
    PgCancelBackend,
    /// `CANCEL QUERY '<id>'` using the id from `SHOW STATEMENTS`.
    CockroachCancelQuery,
    /// Close the connection running the statement.
    CloseConnection,
}

/// Feature matrix the UI and usecases consult instead of branching on `Engine`.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub foreign_keys: bool,
    pub triggers: bool,
    pub extensions: bool,
    pub sequences: bool,
    pub materialized_views: bool,
    pub routines: bool,
    pub server_side_cursors: bool,
    pub explain_json: bool,
    pub explain_analyze: bool,
    pub read_only_sessions: bool,
    /// `None` = unlimited.
    pub max_ddl_per_transaction: Option<u32>,
    pub mixed_ddl_dml_transactions: bool,
    /// `None` = unlimited.
    pub max_rows_mutated_per_transaction: Option<u32>,
    pub async_index_creation: bool,
    pub cancel: CancelStrategy,
}

static POSTGRES: Capabilities = Capabilities {
    foreign_keys: true,
    triggers: true,
    extensions: true,
    sequences: true,
    materialized_views: true,
    routines: true,
    server_side_cursors: true,
    explain_json: true,
    explain_analyze: true,
    read_only_sessions: true,
    max_ddl_per_transaction: None,
    mixed_ddl_dml_transactions: true,
    max_rows_mutated_per_transaction: None,
    async_index_creation: false,
    cancel: CancelStrategy::PgCancelBackend,
};

static AURORA_DSQL: Capabilities = Capabilities {
    // FOREIGN KEY/REFERENCES are listed as supported (inline at CREATE TABLE) in
    // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-postgresql-compatibility.html
    foreign_keys: true,
    triggers: false,
    extensions: false,
    sequences: true,
    materialized_views: false,
    routines: true,
    server_side_cursors: false,
    explain_json: true,
    explain_analyze: true,
    read_only_sessions: true,
    max_ddl_per_transaction: Some(1),
    mixed_ddl_dml_transactions: false,
    max_rows_mutated_per_transaction: Some(3000),
    async_index_creation: true,
    cancel: CancelStrategy::CloseConnection,
};

static COCKROACH_DB: Capabilities = Capabilities {
    foreign_keys: true,
    triggers: true,
    extensions: false,
    sequences: true,
    materialized_views: true,
    routines: true,
    server_side_cursors: true,
    explain_json: false,
    explain_analyze: true,
    read_only_sessions: true,
    max_ddl_per_transaction: None,
    mixed_ddl_dml_transactions: true,
    max_rows_mutated_per_transaction: None,
    async_index_creation: false,
    cancel: CancelStrategy::CockroachCancelQuery,
};

static YUGABYTE_DB: Capabilities = Capabilities {
    foreign_keys: true,
    triggers: true,
    extensions: true,
    sequences: true,
    materialized_views: true,
    routines: true,
    server_side_cursors: true,
    explain_json: true,
    explain_analyze: true,
    read_only_sessions: true,
    max_ddl_per_transaction: None,
    mixed_ddl_dml_transactions: true,
    max_rows_mutated_per_transaction: None,
    async_index_creation: false,
    cancel: CancelStrategy::PgCancelBackend,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_prefers_dsql_host_over_version() {
        assert_eq!(
            Engine::detect("PostgreSQL 16", "abc123.dsql.us-east-1.on.aws"),
            Some(Engine::AuroraDsql)
        );
    }

    #[test]
    fn detect_distributed_engines_from_version() {
        assert_eq!(
            Engine::detect(
                "CockroachDB CCL v25.2.0 (aarch64-unknown-linux-gnu)",
                "localhost"
            ),
            Some(Engine::CockroachDb)
        );
        assert_eq!(
            Engine::detect("PostgreSQL 15.12-YB-2025.1.0.0-b0 on aarch64", "localhost"),
            Some(Engine::YugabyteDb)
        );
        assert_eq!(
            Engine::detect("PostgreSQL 18.0 on aarch64", "db.local"),
            None
        );
    }
}
