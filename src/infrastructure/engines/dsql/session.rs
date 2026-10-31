//! Connect + cancel for Aurora DSQL.

use sqlx::{PgConnection, PgPool};

use crate::domain::database::{ConnectTarget, DbError};
use crate::infrastructure::engines::{CancelHandle, gateway};

pub(super) async fn connect(target: &ConnectTarget) -> Result<PgPool, DbError> {
    gateway::connect_dsql_pool(target).await
}

pub(super) fn cancel_handle(_conn: &mut PgConnection) -> CancelHandle {
    CancelHandle::CloseConnection
}

pub(super) const fn cancel(_pool: &PgPool, _handle: &CancelHandle) -> bool {
    false
}
