//! SQL behind `Engine::YugabyteDb.views()`.

pub(super) fn view_sql(view_id: &str) -> Option<&'static str> {
    match view_id {
        "servers" => Some("SELECT * FROM yb_servers()"),
        "activity" => Some(
            "SELECT pid, usename, application_name, client_addr, state, wait_event_type, \
             wait_event, query_start, query \
             FROM pg_stat_activity \
             WHERE datname = current_database() \
             ORDER BY query_start DESC NULLS LAST",
        ),
        "tablets" => Some(
            "SELECT tablet_id, table_id, table_type, namespace_name, ysql_schema_name, \
             table_name, state \
             FROM pg_catalog.yb_local_tablets \
             WHERE namespace_name = current_database() \
               AND ysql_schema_name = $1 AND table_name = $2 \
             ORDER BY tablet_id",
        ),
        "table_properties" => {
            Some("SELECT * FROM yb_table_properties(format('%I.%I', $1, $2)::regclass)")
        }
        _ => None,
    }
}
