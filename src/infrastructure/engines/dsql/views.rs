//! SQL behind `Engine::AuroraDsql.views()`.

pub(super) fn view_sql(view_id: &str) -> Option<&'static str> {
    match view_id {
        "jobs" => Some("SELECT * FROM sys.jobs"),
        _ => None,
    }
}
