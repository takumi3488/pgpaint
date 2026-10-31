//! EXPLAIN for `YugabyteDB`.
use crate::domain::database::{ExplainResult, StatementResult};

pub(super) fn explain_sql(sql: &str, analyze: bool) -> String {
    if analyze {
        format!("EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) {sql}")
    } else {
        format!("EXPLAIN (FORMAT JSON) {sql}")
    }
}

pub(super) fn parse_plan(output: &StatementResult, analyzed: bool) -> ExplainResult {
    super::super::plan::parse_json(output, analyzed)
}
