//! Connect + cancel for `CockroachDB`.

use sqlx::{AssertSqlSafe, PgConnection, PgPool, raw_sql};

use crate::domain::database::{ConnectTarget, DbError};
use crate::infrastructure::engines::{CancelHandle, gateway};

pub(super) async fn connect(target: &ConnectTarget) -> Result<PgPool, DbError> {
    gateway::connect_password_pool(target).await
}

pub(super) async fn cancel_handle(conn: &mut PgConnection) -> Result<CancelHandle, DbError> {
    let session_id = sqlx::query_scalar::<_, String>("SHOW session_id")
        .fetch_one(conn)
        .await
        .map_err(gateway::map_sqlx_error)?;
    Ok(CancelHandle::CockroachSession(session_id))
}

pub(super) async fn cancel(pool: &PgPool, handle: &CancelHandle) -> Result<bool, DbError> {
    let CancelHandle::CockroachSession(session_id) = handle else {
        return Ok(false);
    };
    let query_ids = sqlx::query_scalar::<_, String>(
        "WITH active AS (SHOW CLUSTER QUERIES) \
         SELECT query_id FROM active WHERE session_id = $1",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(gateway::map_sqlx_error)?;
    let mut canceled = false;
    for query_id in query_ids {
        let sql = format!("CANCEL QUERY '{}'", query_id.replace('\'', "''"));
        let result = raw_sql(AssertSqlSafe(sql))
            .execute(pool)
            .await
            .map_err(gateway::map_sqlx_error)?;
        canceled |= result.rows_affected() > 0;
    }
    Ok(canceled)
}
