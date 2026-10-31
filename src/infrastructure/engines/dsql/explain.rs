//! EXPLAIN for Aurora DSQL.
use crate::domain::database::{ExplainResult, StatementResult};

pub(super) fn explain_sql(sql: &str, analyze: bool) -> String {
    if analyze {
        format!("EXPLAIN ANALYZE VERBOSE {sql}")
    } else {
        format!("EXPLAIN VERBOSE {sql}")
    }
}

pub(super) fn parse_plan(output: &StatementResult, analyzed: bool) -> ExplainResult {
    super::super::plan::parse_text(output, analyzed)
}
