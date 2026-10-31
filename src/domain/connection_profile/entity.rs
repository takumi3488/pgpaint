use super::value_objects::{ConnectionProfileAuth, ConnectionProfileId};
use crate::domain::database::{ConnectAuth, ConnectTarget, Engine, Secret, SslMode};
use gluon::FieldError;
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct ConnectionProfile {
    pub id: ConnectionProfileId,
    pub owner: Option<String>,
    pub name: String,
    pub engine: Engine,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub auth: ConnectionProfileAuth,
    pub ssl_mode: SslMode,
    pub ssl_root_cert_pem: Option<String>,
    pub read_only: bool,
    pub statement_timeout_ms: Option<u64>,
    pub color: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl ConnectionProfile {
    /// Creates a profile with current creation and update timestamps.
    ///
    /// # Errors
    /// Returns validation errors for an empty name or host, an invalid port, or invalid DSQL authentication.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ConnectionProfileId,
        owner: Option<String>,
        name: String,
        engine: Engine,
        host: String,
        port: u32,
        database: String,
        username: String,
        auth: ConnectionProfileAuth,
        ssl_mode: SslMode,
        ssl_root_cert_pem: Option<String>,
        read_only: bool,
        statement_timeout_ms: Option<u64>,
        color: Option<String>,
    ) -> Result<Self, gluon::AppError> {
        let port = u16::try_from(port).unwrap_or_default();
        let now = OffsetDateTime::now_utc();
        let profile = Self {
            id,
            owner,
            name,
            engine,
            host,
            port,
            database,
            username,
            auth,
            ssl_mode,
            ssl_root_cert_pem,
            read_only,
            statement_timeout_ms,
            color,
            created_at: now,
            updated_at: now,
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Checks the profile's required endpoint and engine-specific authentication fields.
    ///
    /// # Errors
    /// Returns [`gluon::AppError::Validation`] with one error for each invalid field.
    pub fn validate(&self) -> Result<(), gluon::AppError> {
        let mut errors = Vec::new();
        if self.name.trim().is_empty() {
            errors.push(FieldError {
                field: "name".to_owned(),
                message: "Name must not be empty.".to_owned(),
            });
        }
        if self.host.trim().is_empty() {
            errors.push(FieldError {
                field: "host".to_owned(),
                message: "Host must not be empty.".to_owned(),
            });
        }
        if self.port == 0 {
            errors.push(FieldError {
                field: "port".to_owned(),
                message: "Port must be between 1 and 65535.".to_owned(),
            });
        }
        match (&self.engine, &self.auth) {
            (Engine::AuroraDsql, ConnectionProfileAuth::DsqlIam { region, .. }) => {
                if region.trim().is_empty() {
                    errors.push(FieldError {
                        field: "auth.region".to_owned(),
                        message: "Aurora DSQL requires an IAM region.".to_owned(),
                    });
                }
            }
            (Engine::AuroraDsql, ConnectionProfileAuth::Password { .. }) => {
                errors.push(FieldError {
                    field: "auth".to_owned(),
                    message: "Aurora DSQL requires IAM authentication.".to_owned(),
                });
            }
            (_, ConnectionProfileAuth::DsqlIam { .. }) => {
                errors.push(FieldError {
                    field: "auth".to_owned(),
                    message: "IAM authentication is only supported for Aurora DSQL.".to_owned(),
                });
            }
            (_, ConnectionProfileAuth::Password { .. }) => {}
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(gluon::AppError::Validation(errors))
        }
    }

    /// Builds the database connection settings, using the decrypted password when required.
    ///
    /// # Errors
    /// Returns validation errors if the profile is invalid or a password is missing.
    pub fn connect_target(&self, secret: Option<Secret>) -> Result<ConnectTarget, gluon::AppError> {
        self.validate()?;
        let auth = match &self.auth {
            ConnectionProfileAuth::Password { .. } => {
                let Some(secret) = secret else {
                    return Err(gluon::AppError::Validation(vec![FieldError {
                        field: "auth.password".to_owned(),
                        message: "A decrypted password is required to connect.".to_owned(),
                    }]));
                };
                ConnectAuth::Password(secret)
            }
            ConnectionProfileAuth::DsqlIam { region, admin } => ConnectAuth::DsqlIam {
                region: region.clone(),
                admin: *admin,
            },
        };
        Ok(ConnectTarget {
            engine: self.engine,
            host: self.host.clone(),
            port: self.port,
            database: self.database.clone(),
            username: self.username.clone(),
            auth,
            ssl_mode: self.ssl_mode,
            ssl_root_cert_pem: self.ssl_root_cert_pem.clone(),
            read_only: self.read_only,
            statement_timeout_ms: self.statement_timeout_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_profile(
        engine: Engine,
        auth: ConnectionProfileAuth,
        name: &str,
        host: &str,
        port: u32,
    ) -> Result<ConnectionProfile, gluon::AppError> {
        ConnectionProfile::new(
            ConnectionProfileId(uuid::Uuid::new_v4()),
            Some("owner".to_owned()),
            name.to_owned(),
            engine,
            host.to_owned(),
            port,
            "database".to_owned(),
            "username".to_owned(),
            auth,
            SslMode::Prefer,
            None,
            false,
            None,
            None,
        )
    }

    #[test]
    fn validates_profile_name_host_port_and_dsql_auth() {
        let result = new_profile(
            Engine::Postgres,
            ConnectionProfileAuth::Password {
                encrypted_secret: None,
            },
            " ",
            " ",
            65_536,
        );
        assert!(matches!(
            result,
            Err(gluon::AppError::Validation(errors))
                if errors.iter().any(|error| error.field == "name")
                    && errors.iter().any(|error| error.field == "host")
                    && errors.iter().any(|error| error.field == "port")
        ));

        let result = new_profile(
            Engine::AuroraDsql,
            ConnectionProfileAuth::Password {
                encrypted_secret: None,
            },
            "dsql",
            "endpoint",
            5432,
        );
        assert!(matches!(
            result,
            Err(gluon::AppError::Validation(errors))
                if errors.iter().any(|error| error.field == "auth")
        ));

        let result = new_profile(
            Engine::AuroraDsql,
            ConnectionProfileAuth::DsqlIam {
                region: " ".to_owned(),
                admin: false,
            },
            "dsql",
            "endpoint",
            5432,
        );
        assert!(matches!(
            result,
            Err(gluon::AppError::Validation(errors))
                if errors.iter().any(|error| error.field == "auth.region")
        ));
    }

    #[test]
    fn builds_password_connect_target_only_with_decrypted_secret() {
        let result = new_profile(
            Engine::Postgres,
            ConnectionProfileAuth::Password {
                encrypted_secret: None,
            },
            "profile",
            "localhost",
            5432,
        );
        assert!(result.is_ok());
        let Some(profile) = result.ok() else {
            return;
        };
        assert!(matches!(
            profile.connect_target(None),
            Err(gluon::AppError::Validation(errors))
                if errors.iter().any(|error| error.field == "auth.password")
        ));
        let target = profile.connect_target(Some(Secret::new("password".to_owned())));
        assert!(matches!(
            &target,
            Ok(target)
                if matches!(&target.auth, ConnectAuth::Password(secret) if secret.expose() == "password")
        ));
    }
}
