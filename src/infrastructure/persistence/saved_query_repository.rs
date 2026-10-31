use crate::domain::connection_profile::ConnectionProfileId;
use crate::domain::saved_query::{SavedQuery, SavedQueryId, SavedQueryRepository};
use async_trait::async_trait;
use sqlx::PgPool;
use sqlx::Row;
use std::sync::Arc;

pub struct PostgresSavedQueryRepository {
    pool: Arc<PgPool>,
}

impl PostgresSavedQueryRepository {
    #[must_use]
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

fn internal(error: impl std::error::Error + Send + Sync + 'static) -> gluon::AppError {
    gluon::AppError::Internal(Box::new(error))
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<SavedQuery, gluon::AppError> {
    let profile_id: Option<uuid::Uuid> = row.try_get("profile_id").map_err(internal)?;
    Ok(SavedQuery {
        id: SavedQueryId(row.try_get("id").map_err(internal)?),
        owner: row.try_get("owner").map_err(internal)?,
        profile_id: profile_id.map(ConnectionProfileId),
        name: row.try_get("name").map_err(internal)?,
        sql: row.try_get("sql").map_err(internal)?,
        created_at: row.try_get("created_at").map_err(internal)?,
        updated_at: row.try_get("updated_at").map_err(internal)?,
    })
}

#[async_trait]
impl SavedQueryRepository for PostgresSavedQueryRepository {
    async fn list(
        &self,
        owner: Option<&str>,
        profile_id: Option<&ConnectionProfileId>,
        limit: u32,
    ) -> Result<Vec<SavedQuery>, gluon::AppError> {
        self.list_search(owner, profile_id, limit, None).await
    }

    async fn list_search(
        &self,
        owner: Option<&str>,
        profile_id: Option<&ConnectionProfileId>,
        limit: u32,
        search: Option<&str>,
    ) -> Result<Vec<SavedQuery>, gluon::AppError> {
        sqlx::query(
            "SELECT \"id\", \"owner\", \"profile_id\", \"name\", \"sql\", \"created_at\", \
             \"updated_at\" FROM \"saved_queries\" \
             WHERE \"owner\" IS NOT DISTINCT FROM $1 \
             AND ($2::uuid IS NULL OR \"profile_id\" = $2) \
             AND ($3::text IS NULL OR position(lower($3) in lower(\"name\")) > 0 \
                  OR position(lower($3) in lower(\"sql\")) > 0) \
             ORDER BY \"updated_at\" DESC, \"id\" DESC LIMIT $4",
        )
        .bind(owner)
        .bind(profile_id.map(|id| id.0))
        .bind(search)
        .bind(i64::from(limit))
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(internal)?
        .iter()
        .map(from_row)
        .collect()
    }

    async fn find(
        &self,
        owner: Option<&str>,
        id: &SavedQueryId,
    ) -> Result<Option<SavedQuery>, gluon::AppError> {
        sqlx::query(
            "SELECT \"id\", \"owner\", \"profile_id\", \"name\", \"sql\", \"created_at\", \
             \"updated_at\" FROM \"saved_queries\" \
             WHERE \"owner\" IS NOT DISTINCT FROM $1 AND \"id\" = $2",
        )
        .bind(owner)
        .bind(id.0)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_err(internal)?
        .as_ref()
        .map(from_row)
        .transpose()
    }

    async fn save(&self, entity: &SavedQuery) -> Result<(), gluon::AppError> {
        let result = sqlx::query(
            "INSERT INTO \"saved_queries\" \
             (\"id\", \"owner\", \"profile_id\", \"name\", \"sql\", \"created_at\", \"updated_at\") \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (\"id\") DO UPDATE SET \
             \"owner\" = EXCLUDED.\"owner\", \"profile_id\" = EXCLUDED.\"profile_id\", \
             \"name\" = EXCLUDED.\"name\", \"sql\" = EXCLUDED.\"sql\", \
             \"updated_at\" = EXCLUDED.\"updated_at\" \
             WHERE \"saved_queries\".\"owner\" IS NOT DISTINCT FROM EXCLUDED.\"owner\"",
        )
        .bind(entity.id.0)
        .bind(entity.owner.as_deref())
        .bind(entity.profile_id.as_ref().map(|id| id.0))
        .bind(&entity.name)
        .bind(&entity.sql)
        .bind(entity.created_at)
        .bind(entity.updated_at)
        .execute(self.pool.as_ref())
        .await
        .map_err(internal)?;
        if result.rows_affected() == 0 {
            return Err(gluon::AppError::Conflict(
                "Saved query ID already belongs to another owner.".to_owned(),
            ));
        }
        Ok(())
    }

    async fn delete(&self, owner: Option<&str>, id: &SavedQueryId) -> Result<(), gluon::AppError> {
        sqlx::query(
            "DELETE FROM \"saved_queries\" \
             WHERE \"owner\" IS NOT DISTINCT FROM $1 AND \"id\" = $2",
        )
        .bind(owner)
        .bind(id.0)
        .execute(self.pool.as_ref())
        .await
        .map_err(internal)?;
        Ok(())
    }
}
