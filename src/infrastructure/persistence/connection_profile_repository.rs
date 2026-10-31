use crate::domain::connection_profile::{
    ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId, ConnectionProfileRepository,
    EncryptedSecret,
};
use crate::domain::database::{Engine, SslMode};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use std::io;
use std::sync::Arc;

pub struct PostgresConnectionProfileRepository {
    pool: Arc<PgPool>,
}

impl PostgresConnectionProfileRepository {
    #[must_use]
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

fn internal(error: impl std::error::Error + Send + Sync + 'static) -> gluon::AppError {
    gluon::AppError::Internal(Box::new(error))
}

fn invalid_data(message: &'static str) -> gluon::AppError {
    internal(io::Error::other(message))
}

const fn engine_name(engine: Engine) -> &'static str {
    match engine {
        Engine::Postgres => "postgres",
        Engine::AuroraDsql => "aurora_dsql",
        Engine::CockroachDb => "cockroach_db",
        Engine::YugabyteDb => "yugabyte_db",
    }
}

fn parse_engine(value: &str) -> Result<Engine, gluon::AppError> {
    match value {
        "postgres" => Ok(Engine::Postgres),
        "aurora_dsql" => Ok(Engine::AuroraDsql),
        "cockroach_db" => Ok(Engine::CockroachDb),
        "yugabyte_db" => Ok(Engine::YugabyteDb),
        _ => Err(invalid_data("invalid connection profile engine")),
    }
}

const fn ssl_mode_name(mode: SslMode) -> &'static str {
    match mode {
        SslMode::Disable => "disable",
        SslMode::Prefer => "prefer",
        SslMode::Require => "require",
        SslMode::VerifyCa => "verify_ca",
        SslMode::VerifyFull => "verify_full",
    }
}

fn parse_ssl_mode(value: &str) -> Result<SslMode, gluon::AppError> {
    match value {
        "disable" => Ok(SslMode::Disable),
        "prefer" => Ok(SslMode::Prefer),
        "require" => Ok(SslMode::Require),
        "verify_ca" => Ok(SslMode::VerifyCa),
        "verify_full" => Ok(SslMode::VerifyFull),
        _ => Err(invalid_data("invalid connection profile SSL mode")),
    }
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<ConnectionProfile, gluon::AppError> {
    let port = row.try_get::<i32, _>("port").map_err(internal)?;
    let port = u16::try_from(port).map_err(internal)?;
    let timeout = row
        .try_get::<Option<i64>, _>("statement_timeout_ms")
        .map_err(internal)?
        .map(u64::try_from)
        .transpose()
        .map_err(internal)?;
    let engine_value: String = row.try_get("engine").map_err(internal)?;
    let engine = parse_engine(&engine_value)?;
    let auth_type: String = row.try_get("auth_type").map_err(internal)?;
    let ssl_mode: String = row.try_get("ssl_mode").map_err(internal)?;
    let encrypted_secret = row
        .try_get::<Option<String>, _>("encrypted_secret")
        .map_err(internal)?
        .map(EncryptedSecret::new);
    let auth = match auth_type.as_str() {
        "password" => ConnectionProfileAuth::Password { encrypted_secret },
        "dsql_iam" => ConnectionProfileAuth::DsqlIam {
            region: row
                .try_get::<Option<String>, _>("iam_region")
                .map_err(internal)?
                .ok_or_else(|| invalid_data("Aurora DSQL profile is missing its IAM region"))?,
            admin: row
                .try_get::<Option<bool>, _>("iam_admin")
                .map_err(internal)?
                .ok_or_else(|| invalid_data("Aurora DSQL profile is missing its IAM admin flag"))?,
        },
        _ => {
            return Err(invalid_data(
                "invalid connection profile authentication type",
            ));
        }
    };
    Ok(ConnectionProfile {
        id: ConnectionProfileId(row.try_get("id").map_err(internal)?),
        owner: row.try_get("owner").map_err(internal)?,
        name: row.try_get("name").map_err(internal)?,
        engine,
        host: row.try_get("host").map_err(internal)?,
        port,
        database: row.try_get("database_name").map_err(internal)?,
        username: row.try_get("username").map_err(internal)?,
        auth,
        ssl_mode: parse_ssl_mode(&ssl_mode)?,
        ssl_root_cert_pem: row.try_get("ssl_root_cert_pem").map_err(internal)?,
        read_only: row.try_get("read_only").map_err(internal)?,
        statement_timeout_ms: timeout,
        color: row.try_get("color").map_err(internal)?,
        created_at: row.try_get("created_at").map_err(internal)?,
        updated_at: row.try_get("updated_at").map_err(internal)?,
    })
}

#[async_trait]
impl ConnectionProfileRepository for PostgresConnectionProfileRepository {
    async fn list(
        &self,
        owner: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ConnectionProfile>, gluon::AppError> {
        let rows = sqlx::query(
            "SELECT id, owner, name, engine, host, port, database_name, username, auth_type, \
             encrypted_secret, iam_region, iam_admin, ssl_mode, ssl_root_cert_pem, read_only, \
             statement_timeout_ms, color, created_at, updated_at \
             FROM connection_profiles WHERE owner IS NOT DISTINCT FROM $1 \
             ORDER BY updated_at DESC, id LIMIT $2",
        )
        .bind(owner)
        .bind(i64::from(limit))
        .fetch_all(self.pool.as_ref())
        .await
        .map_err(internal)?;
        rows.iter().map(from_row).collect()
    }

    async fn find(
        &self,
        owner: Option<&str>,
        id: &ConnectionProfileId,
    ) -> Result<Option<ConnectionProfile>, gluon::AppError> {
        sqlx::query(
            "SELECT id, owner, name, engine, host, port, database_name, username, auth_type, \
             encrypted_secret, iam_region, iam_admin, ssl_mode, ssl_root_cert_pem, read_only, \
             statement_timeout_ms, color, created_at, updated_at \
             FROM connection_profiles WHERE owner IS NOT DISTINCT FROM $1 AND id = $2",
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

    async fn save(&self, entity: &ConnectionProfile) -> Result<(), gluon::AppError> {
        entity.validate()?;
        let timeout = entity
            .statement_timeout_ms
            .map(i64::try_from)
            .transpose()
            .map_err(|_| {
                gluon::AppError::Validation(vec![gluon::FieldError {
                    field: "statement_timeout_ms".to_owned(),
                    message: "Statement timeout must fit in a signed 64-bit integer.".to_owned(),
                }])
            })?;
        let (auth_type, encrypted_secret, iam_region, iam_admin) = match &entity.auth {
            ConnectionProfileAuth::Password { encrypted_secret } => (
                "password",
                encrypted_secret.as_ref().map(EncryptedSecret::expose),
                None,
                None,
            ),
            ConnectionProfileAuth::DsqlIam { region, admin } => {
                ("dsql_iam", None, Some(region.as_str()), Some(*admin))
            }
        };
        let result = sqlx::query(
            "INSERT INTO connection_profiles \
             (id, owner, name, engine, host, port, database_name, username, auth_type, \
              encrypted_secret, iam_region, iam_admin, ssl_mode, ssl_root_cert_pem, read_only, \
              statement_timeout_ms, color, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19) \
             ON CONFLICT (id) DO UPDATE SET \
              name = EXCLUDED.name, engine = EXCLUDED.engine, host = EXCLUDED.host, \
              port = EXCLUDED.port, database_name = EXCLUDED.database_name, \
              username = EXCLUDED.username, auth_type = EXCLUDED.auth_type, \
              encrypted_secret = EXCLUDED.encrypted_secret, iam_region = EXCLUDED.iam_region, \
              iam_admin = EXCLUDED.iam_admin, ssl_mode = EXCLUDED.ssl_mode, \
              ssl_root_cert_pem = EXCLUDED.ssl_root_cert_pem, read_only = EXCLUDED.read_only, \
              statement_timeout_ms = EXCLUDED.statement_timeout_ms, color = EXCLUDED.color, \
              updated_at = NOW() \
             WHERE connection_profiles.owner IS NOT DISTINCT FROM EXCLUDED.owner",
        )
        .bind(entity.id.0)
        .bind(entity.owner.as_deref())
        .bind(&entity.name)
        .bind(engine_name(entity.engine))
        .bind(&entity.host)
        .bind(i32::from(entity.port))
        .bind(&entity.database)
        .bind(&entity.username)
        .bind(auth_type)
        .bind(encrypted_secret)
        .bind(iam_region)
        .bind(iam_admin)
        .bind(ssl_mode_name(entity.ssl_mode))
        .bind(&entity.ssl_root_cert_pem)
        .bind(entity.read_only)
        .bind(timeout)
        .bind(&entity.color)
        .bind(entity.created_at)
        .bind(entity.updated_at)
        .execute(self.pool.as_ref())
        .await
        .map_err(internal)?;
        if result.rows_affected() == 0 {
            return Err(gluon::AppError::Conflict(
                "Connection profile ID already belongs to another owner.".to_owned(),
            ));
        }
        Ok(())
    }

    async fn delete(
        &self,
        owner: Option<&str>,
        id: &ConnectionProfileId,
    ) -> Result<(), gluon::AppError> {
        sqlx::query(
            "DELETE FROM connection_profiles WHERE owner IS NOT DISTINCT FROM $1 AND id = $2",
        )
        .bind(owner)
        .bind(id.0)
        .execute(self.pool.as_ref())
        .await
        .map_err(internal)?;
        Ok(())
    }
}
