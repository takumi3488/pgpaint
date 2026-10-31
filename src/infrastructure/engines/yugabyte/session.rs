//! Connect + cancel for `YugabyteDB`.

use sqlx::{PgConnection, PgPool};

use crate::domain::database::{ConnectTarget, DbError};
use crate::infrastructure::engines::{CancelHandle, gateway};

pub(super) async fn connect(target: &ConnectTarget) -> Result<PgPool, DbError> {
    gateway::connect_password_pool(target).await
}

pub(super) async fn cancel_handle(conn: &mut PgConnection) -> Result<CancelHandle, DbError> {
    let pid = sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
        .fetch_one(conn)
        .await
        .map_err(gateway::map_sqlx_error)?;
    Ok(CancelHandle::BackendPid(pid))
}

pub(super) async fn cancel(pool: &PgPool, handle: &CancelHandle) -> Result<bool, DbError> {
    let CancelHandle::BackendPid(pid) = handle else {
        return Ok(false);
    };
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE pid = $1 AND state = 'active')",
    )
    .bind(pid)
    .fetch_one(pool)
    .await
    .map_err(gateway::map_sqlx_error)?;
    if !active {
        return Ok(false);
    }
    gateway::cancel_postgres(pool, handle).await
}
