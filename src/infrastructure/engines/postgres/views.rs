//! SQL behind `Engine::Postgres.views()`.

pub(super) fn view_sql(view_id: &str) -> Option<&'static str> {
    match view_id {
        "activity" => Some(
            "SELECT pid, usename, application_name, client_addr, state, wait_event_type, \
             wait_event, query_start, query \
             FROM pg_stat_activity \
             WHERE datname = current_database() \
             ORDER BY query_start DESC NULLS LAST",
        ),
        "locks" => Some(
            "SELECT l.locktype, n.nspname AS schema_name, c.relname AS relation_name, \
             l.mode, l.granted, a.pid, a.usename, a.application_name, a.query \
             FROM pg_locks AS l \
             LEFT JOIN pg_stat_activity AS a ON a.pid = l.pid \
             LEFT JOIN pg_class AS c ON c.oid = l.relation \
             LEFT JOIN pg_namespace AS n ON n.oid = c.relnamespace \
             ORDER BY l.granted, a.pid",
        ),
        "table_stats" => Some(
            "SELECT schemaname, relname, seq_scan, seq_tup_read, idx_scan, idx_tup_fetch, \
             n_tup_ins, n_tup_upd, n_tup_del, n_live_tup, n_dead_tup, \
             last_vacuum, last_autovacuum, last_analyze, last_autoanalyze \
             FROM pg_stat_user_tables \
             WHERE schemaname::text = $1 AND relname::text = $2",
        ),
        _ => None,
    }
}
