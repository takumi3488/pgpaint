//! Connect + cancel for PostgreSQL.

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
    gateway::cancel_postgres(pool, handle).await
}
