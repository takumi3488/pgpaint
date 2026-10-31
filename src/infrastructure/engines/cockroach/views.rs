//! SQL behind `Engine::CockroachDb.views()`.

pub(super) fn view_sql(view_id: &str) -> Option<&'static str> {
    match view_id {
        "regions" => Some("SHOW REGIONS"),
        "jobs" => Some("SHOW JOBS"),
        "sessions" => Some("SHOW SESSIONS"),
        "ranges" => Some(
            "WITH ranges AS (SHOW RANGES WITH TABLES) \
             SELECT * FROM ranges \
             WHERE schema_name = $1 AND table_name = $2 \
             ORDER BY range_id",
        ),
        _ => None,
    }
}
