use crate::domain::connection_profile::ConnectionProfileId;
use crate::domain::query_history::{
    QueryHistory, QueryHistoryId, QueryHistoryRepository, QueryHistoryStatus,
};
use async_trait::async_trait;
use sqlx::PgPool;
use sqlx::Row;
use std::sync::Arc;

pub struct PostgresQueryHistoryRepository {
    pool: Arc<PgPool>,
}

impl PostgresQueryHistoryRepository {
    #[must_use]
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

fn internal(error: impl std::error::Error + Send + Sync + 'static) -> gluon::AppError {
    gluon::AppError::Internal(Box::new(error))
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<QueryHistory, gluon::AppError> {
    let elapsed_ms =
        u64::try_from(row.try_get::<i64, _>("elapsed_ms").map_err(internal)?).map_err(internal)?;
    let rows = row
        .try_get::<Option<i64>, _>("rows")
        .map_err(internal)?
        .map(u64::try_from)
        .transpose()
        .map_err(internal)?;
    let rows_affected = row
        .try_get::<Option<i64>, _>("rows_affected")
        .map_err(internal)?
        .map(u64::try_from)
        .transpose()
        .map_err(internal)?;
    let status: String = row.try_get("status").map_err(internal)?;
    let status = match status.as_str() {
        "succeeded" => QueryHistoryStatus::Succeeded,
        "failed" => QueryHistoryStatus::Failed,
        "canceled" => QueryHistoryStatus::Canceled,
        _ => {
            return Err(internal(std::io::Error::other(
                "invalid query history status",
            )));
        }
    };

    Ok(QueryHistory {
        id: QueryHistoryId(row.try_get("id").map_err(internal)?),
        owner: row.try_get("owner").map_err(internal)?,
        profile_id: ConnectionProfileId(row.try_get("profile_id").map_err(internal)?),
        sql: row.try_get("sql").map_err(internal)?,
        executed_at: row.try_get("executed_at").map_err(internal)?,
        elapsed_ms,
        rows,
        rows_affected,
        status,
        error_message: row.try_get("error_message").map_err(internal)?,
    })
}

const fn status_as_str(status: QueryHistoryStatus) -> &'static str {
    match status {
        QueryHistoryStatus::Succeeded => "succeeded",
        QueryHistoryStatus::Failed => "failed",
        QueryHistoryStatus::Canceled => "canceled",
    }
}

#[async_trait]
impl QueryHistoryRepository for PostgresQueryHistoryRepository {
    async fn list(
        &self,
        owner: Option<&str>,
        profile_id: &ConnectionProfileId,
        offset: u64,
        limit: u32,
        search: Option<&str>,
    ) -> Result<Vec<QueryHistory>, gluon::AppError> {
        self.list_filtered(owner, Some(profile_id), offset, limit, search, None)
            .await
    }

    async fn list_filtered(
        &self,
        owner: Option<&str>,
        profile_id: Option<&ConnectionProfileId>,
        offset: u64,
        limit: u32,
        search: Option<&str>,
        status: Option<QueryHistoryStatus>,
    ) -> Result<Vec<QueryHistory>, gluon::AppError> {
        let offset = i64::try_from(offset).map_err(internal)?;
        sqlx::query(
            "SELECT \"id\", \"owner\", \"profile_id\", \"sql\", \"executed_at\", \
             \"elapsed_ms\", \"rows\", \"rows_affected\", \"status\", \"error_message\" \
             FROM \"query_history\" \
             WHERE \"owner\" IS NOT DISTINCT FROM $1 \
             AND ($2::uuid IS NULL OR \"profile_id\" = $2) \
             AND ($3::text IS NULL OR position(lower($3) in lower(\"sql\")) > 0) \
             AND ($4::text IS NULL OR \"status\" = $4) \
             ORDER BY \"executed_at\" DESC, \"id\" DESC OFFSET $5 LIMIT $6",
        )
        .bind(owner)
        .bind(profile_id.map(|id| id.0))
        .bind(search)
        .bind(status.map(status_as_str))
        .bind(offset)
        .bind(i64::from(limit))
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(internal)?
        .iter()
        .map(from_row)
        .collect()
    }

    async fn count(
        &self,
        owner: Option<&str>,
        profile_id: Option<&ConnectionProfileId>,
        search: Option<&str>,
        status: Option<QueryHistoryStatus>,
    ) -> Result<u64, gluon::AppError> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS \"count\" FROM \"query_history\" \
             WHERE \"owner\" IS NOT DISTINCT FROM $1 \
             AND ($2::uuid IS NULL OR \"profile_id\" = $2) \
             AND ($3::text IS NULL OR position(lower($3) in lower(\"sql\")) > 0) \
             AND ($4::text IS NULL OR \"status\" = $4)",
        )
        .bind(owner)
        .bind(profile_id.map(|id| id.0))
        .bind(search)
        .bind(status.map(status_as_str))
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(internal)?;
        u64::try_from(row.try_get::<i64, _>("count").map_err(internal)?).map_err(internal)
    }

    async fn find(
        &self,
        owner: Option<&str>,
        profile_id: &ConnectionProfileId,
        id: &crate::domain::query_history::QueryHistoryId,
    ) -> Result<Option<QueryHistory>, gluon::AppError> {
        sqlx::query(
            "SELECT \"id\", \"owner\", \"profile_id\", \"sql\", \"executed_at\", \
             \"elapsed_ms\", \"rows\", \"rows_affected\", \"status\", \"error_message\" \
             FROM \"query_history\" WHERE \"owner\" IS NOT DISTINCT FROM $1 \
             AND \"profile_id\" = $2 AND \"id\" = $3",
        )
        .bind(owner)
        .bind(profile_id.0)
        .bind(id.0)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(internal)?
        .as_ref()
        .map(from_row)
        .transpose()
    }

    async fn delete(
        &self,
        owner: Option<&str>,
        profile_id: Option<&ConnectionProfileId>,
        id: &crate::domain::query_history::QueryHistoryId,
    ) -> Result<(), gluon::AppError> {
        sqlx::query(
            "DELETE FROM \"query_history\" \
             WHERE \"owner\" IS NOT DISTINCT FROM $1 \
             AND ($2::uuid IS NULL OR \"profile_id\" = $2) AND \"id\" = $3",
        )
        .bind(owner)
        .bind(profile_id.map(|profile| profile.0))
        .bind(id.0)
        .execute(self.pool.as_ref())
        .await
        .map_err(internal)?;
        Ok(())
    }

    async fn save(&self, entity: &QueryHistory) -> Result<(), gluon::AppError> {
        let elapsed_ms = i64::try_from(entity.elapsed_ms).map_err(internal)?;
        let rows = entity
            .rows
            .map(i64::try_from)
            .transpose()
            .map_err(internal)?;
        let rows_affected = entity
            .rows_affected
            .map(i64::try_from)
            .transpose()
            .map_err(internal)?;
        let result = sqlx::query(
            "INSERT INTO \"query_history\" \
             (\"id\", \"owner\", \"profile_id\", \"sql\", \"executed_at\", \"elapsed_ms\", \
              \"rows\", \"rows_affected\", \"status\", \"error_message\") \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
             ON CONFLICT (\"id\") DO UPDATE SET \
             \"owner\" = EXCLUDED.\"owner\", \"profile_id\" = EXCLUDED.\"profile_id\", \
             \"sql\" = EXCLUDED.\"sql\", \"executed_at\" = EXCLUDED.\"executed_at\", \
             \"elapsed_ms\" = EXCLUDED.\"elapsed_ms\", \"rows\" = EXCLUDED.\"rows\", \
             \"rows_affected\" = EXCLUDED.\"rows_affected\", \"status\" = EXCLUDED.\"status\", \
             \"error_message\" = EXCLUDED.\"error_message\" \
             WHERE \"query_history\".\"owner\" IS NOT DISTINCT FROM EXCLUDED.\"owner\" \
             AND \"query_history\".\"profile_id\" = EXCLUDED.\"profile_id\"",
        )
        .bind(entity.id.0)
        .bind(entity.owner.as_deref())
        .bind(entity.profile_id.0)
        .bind(&entity.sql)
        .bind(entity.executed_at)
        .bind(elapsed_ms)
        .bind(rows)
        .bind(rows_affected)
        .bind(status_as_str(entity.status))
        .bind(entity.error_message.as_deref())
        .execute(self.pool.as_ref())
        .await
        .map_err(internal)?;
        if result.rows_affected() == 0 {
            return Err(gluon::AppError::Conflict(
                "History ID already belongs to another owner or profile.".to_owned(),
            ));
        }
        Ok(())
    }

    async fn prune(
        &self,
        owner: Option<&str>,
        profile_id: &ConnectionProfileId,
        keep_newest: u32,
    ) -> Result<(), gluon::AppError> {
        sqlx::query(
            "DELETE FROM \"query_history\" \
             WHERE \"owner\" IS NOT DISTINCT FROM $1 AND \"profile_id\" = $2 \
             AND \"id\" NOT IN ( \
                 SELECT \"id\" FROM \"query_history\" \
                 WHERE \"owner\" IS NOT DISTINCT FROM $1 AND \"profile_id\" = $2 \
                 ORDER BY \"executed_at\" DESC, \"id\" DESC LIMIT $3 \
             )",
        )
        .bind(owner)
        .bind(profile_id.0)
        .bind(i64::from(keep_newest))
        .execute(self.pool.as_ref())
        .await
        .map_err(internal)?;
        Ok(())
    }
}
