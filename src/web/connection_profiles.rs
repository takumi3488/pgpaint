use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use gluon::View;

use crate::domain::{
    connection_profile::{
        ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId, EncryptedSecret,
        SecretCipher, SecretCipherError,
    },
    database::{Engine, Secret, SslMode},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ConnectionProfileInput {
    #[serde(rename = "_action")]
    pub action: Option<String>,
    pub source_profile_id: Option<String>,
    pub name: String,
    pub engine: String,
    pub host: String,
    pub port: String,
    pub database: String,
    pub username: String,
    pub password: String,
    pub save_password: Option<String>,
    pub clear_saved_password: Option<String>,
    pub ssl_mode: String,
    pub ssl_root_cert_pem: String,
    pub read_only: Option<String>,
    pub statement_timeout_ms: String,
    pub color: String,
    pub clear_color: Option<String>,
    pub iam_region: String,
    pub iam_admin: Option<String>,
}

#[derive(Serialize)]
pub struct ConnectionEditorProps {
    #[serde(flatten)]
    pub shell: super::Shell,
    pub action: String,
    pub is_edit: bool,
    pub has_secret_cipher: bool,
    pub values: ConnectionProfileValues,
    pub errors: ConnectionProfileFieldErrors,
}

#[allow(clippy::struct_excessive_bools)] // This serialized form model exposes independent selection and checkbox states.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionProfileValues {
    pub name: String,
    pub postgres_selected: bool,
    pub dsql_selected: bool,
    pub cockroach_selected: bool,
    pub yugabyte_selected: bool,
    pub host: String,
    pub port: String,
    pub database: String,
    pub username: String,
    pub ssl_disable_selected: bool,
    pub ssl_prefer_selected: bool,
    pub ssl_require_selected: bool,
    pub ssl_verify_ca_selected: bool,
    pub ssl_verify_full_selected: bool,
    pub ssl_root_cert_pem: String,
    pub read_only: bool,
    pub statement_timeout_ms: String,
    pub color: String,
    pub clear_color: bool,
    pub iam_region: String,
    pub iam_admin: bool,
    pub source_profile_id: String,
    pub has_saved_password: bool,
    pub save_password: bool,
    pub clear_saved_password: bool,
}

impl ConnectionProfileValues {
    #[must_use]
    pub fn empty() -> Self {
        Self::new(
            String::new(),
            "postgres",
            String::new(),
            "5432".to_owned(),
            String::new(),
            String::new(),
            "prefer",
            String::new(),
            false,
            String::new(),
            "#0ea5e9".to_owned(),
            false,
            String::new(),
            false,
            String::new(),
            false,
            false,
            false,
        )
    }

    #[must_use]
    pub fn from_input(input: &ConnectionProfileInput, has_saved_password: bool) -> Self {
        Self::new(
            input.name.clone(),
            &input.engine,
            input.host.clone(),
            input.port.clone(),
            input.database.clone(),
            input.username.clone(),
            &input.ssl_mode,
            input.ssl_root_cert_pem.clone(),
            input.read_only.is_some(),
            input.statement_timeout_ms.clone(),
            input.color.clone(),
            input.clear_color.is_some(),
            input.iam_region.clone(),
            input.iam_admin.is_some(),
            input.source_profile_id.clone().unwrap_or_default(),
            has_saved_password,
            input.save_password.is_some(),
            input.clear_saved_password.is_some(),
        )
    }

    #[must_use]
    pub fn from_profile(
        profile: &ConnectionProfile,
        source_profile_id: &ConnectionProfileId,
        duplicate: bool,
    ) -> Self {
        let (iam_region, iam_admin, has_saved_password) = match &profile.auth {
            ConnectionProfileAuth::Password { encrypted_secret } => {
                (String::new(), false, encrypted_secret.is_some())
            }
            ConnectionProfileAuth::DsqlIam { region, admin } => (region.clone(), *admin, false),
        };
        Self::new(
            if duplicate {
                format!("{} copy", profile.name)
            } else {
                profile.name.clone()
            },
            engine_name(profile.engine),
            profile.host.clone(),
            profile.port.to_string(),
            profile.database.clone(),
            profile.username.clone(),
            ssl_mode_name(profile.ssl_mode),
            profile.ssl_root_cert_pem.clone().unwrap_or_default(),
            profile.read_only,
            profile
                .statement_timeout_ms
                .map_or_else(String::new, |timeout| timeout.to_string()),
            profile
                .color
                .clone()
                .unwrap_or_else(|| "#0ea5e9".to_owned()),
            profile.color.is_none(),
            iam_region,
            iam_admin,
            source_profile_id.0.to_string(),
            has_saved_password,
            false,
            false,
        )
    }

    #[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
    fn new(
        name: String,
        engine: &str,
        host: String,
        port: String,
        database: String,
        username: String,
        ssl_mode: &str,
        ssl_root_cert_pem: String,
        read_only: bool,
        statement_timeout_ms: String,
        color: String,
        clear_color: bool,
        iam_region: String,
        iam_admin: bool,
        source_profile_id: String,
        has_saved_password: bool,
        save_password: bool,
        clear_saved_password: bool,
    ) -> Self {
        let postgres_selected = engine == "postgres";
        let dsql_selected = engine == "aurora_dsql";
        let cockroach_selected = engine == "cockroach_db";
        let yugabyte_selected = engine == "yugabyte_db";
        let ssl_disable_selected = ssl_mode == "disable";
        let ssl_prefer_selected = ssl_mode == "prefer";
        let ssl_require_selected = ssl_mode == "require";
        let ssl_verify_ca_selected = ssl_mode == "verify-ca";
        let ssl_verify_full_selected = ssl_mode == "verify-full";
        Self {
            name,
            postgres_selected,
            dsql_selected,
            cockroach_selected,
            yugabyte_selected,
            host,
            port,
            database,
            username,
            ssl_disable_selected,
            ssl_prefer_selected,
            ssl_require_selected,
            ssl_verify_ca_selected,
            ssl_verify_full_selected,
            ssl_root_cert_pem,
            read_only,
            statement_timeout_ms,
            color,
            clear_color,
            iam_region,
            iam_admin,
            source_profile_id,
            has_saved_password,
            save_password,
            clear_saved_password,
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct ConnectionProfileFieldErrors {
    pub name: Option<String>,
    pub engine: Option<String>,
    pub host: Option<String>,
    pub port: Option<String>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub ssl_mode: Option<String>,
    pub ssl_root_cert_pem: Option<String>,
    pub read_only: Option<String>,
    pub statement_timeout_ms: Option<String>,
    pub color: Option<String>,
    pub iam_region: Option<String>,
    pub general: Option<String>,
}

impl ConnectionProfileFieldErrors {
    #[must_use]
    pub fn from_fields(fields: Vec<gluon::FieldError>) -> Self {
        let mut result = Self::default();
        for field in fields {
            let target = match field.field.as_str() {
                "name" => &mut result.name,
                "engine" => &mut result.engine,
                "host" => &mut result.host,
                "port" => &mut result.port,
                "database" => &mut result.database,
                "username" => &mut result.username,
                "auth.password" | "auth" => &mut result.password,
                "ssl_mode" => &mut result.ssl_mode,
                "ssl_root_cert_pem" => &mut result.ssl_root_cert_pem,
                "read_only" => &mut result.read_only,
                "statement_timeout_ms" => &mut result.statement_timeout_ms,
                "color" => &mut result.color,
                "auth.region" => &mut result.iam_region,
                _ => &mut result.general,
            };
            if let Some(message) = target {
                message.push(' ');
                message.push_str(&field.message);
            } else {
                *target = Some(field.message);
            }
        }
        result
    }
}

struct ParsedConnectionFields {
    engine: Engine,
    port: u32,
    ssl_mode: SslMode,
    statement_timeout_ms: Option<u64>,
    color: Option<String>,
}

fn parse_connection_fields(
    input: &ConnectionProfileInput,
) -> Result<ParsedConnectionFields, Vec<gluon::FieldError>> {
    let mut errors = Vec::new();
    let engine = parse_engine(&input.engine).unwrap_or_else(|| {
        errors.push(field_error("engine", "Choose a supported database engine."));
        Engine::Postgres
    });
    let port = input.port.parse::<u32>().unwrap_or_else(|_| {
        errors.push(field_error("port", "Enter a port between 1 and 65535."));
        0
    });
    let ssl_mode = parse_ssl_mode(&input.ssl_mode).unwrap_or_else(|| {
        errors.push(field_error("ssl_mode", "Choose a supported SSL mode."));
        SslMode::Prefer
    });
    let statement_timeout_ms = if input.statement_timeout_ms.trim().is_empty() {
        None
    } else {
        match input.statement_timeout_ms.parse::<u64>() {
            Ok(timeout) if i64::try_from(timeout).is_ok() => Some(timeout),
            _ => {
                errors.push(field_error(
                    "statement_timeout_ms",
                    "Enter a non-negative timeout that fits in a signed 64-bit integer.",
                ));
                None
            }
        }
    };
    let color = if input.clear_color.is_some() || input.color.trim().is_empty() {
        None
    } else if is_hex_color(&input.color) {
        Some(input.color.clone())
    } else {
        errors.push(field_error("color", "Choose a valid six-digit color."));
        None
    };
    if port > u32::from(u16::MAX) {
        errors.push(field_error("port", "Port must be between 1 and 65535."));
    }
    if errors.is_empty() {
        Ok(ParsedConnectionFields {
            engine,
            port,
            ssl_mode,
            statement_timeout_ms,
            color,
        })
    } else {
        Err(errors)
    }
}

/// Builds a validated profile and encrypts any password selected for storage.
///
/// # Errors
/// Returns validation errors for malformed connection fields or an unavailable cipher,
/// and an internal error if encryption fails unexpectedly.
pub fn build_profile(
    owner: Option<String>,
    id: ConnectionProfileId,
    input: &ConnectionProfileInput,
    previous_secret: Option<EncryptedSecret>,
    cipher: &dyn SecretCipher,
) -> Result<ConnectionProfile, gluon::AppError> {
    let fields = parse_connection_fields(input).map_err(gluon::AppError::Validation)?;
    let auth = if fields.engine == Engine::AuroraDsql {
        ConnectionProfileAuth::DsqlIam {
            region: input.iam_region.clone(),
            admin: input.iam_admin.is_some(),
        }
    } else {
        let encrypted_secret = if input.clear_saved_password.is_some() {
            None
        } else if input.save_password.is_some() {
            if input.password.is_empty() {
                return Err(gluon::AppError::Validation(vec![field_error(
                    "auth.password",
                    "Enter a password to save it.",
                )]));
            }
            match cipher.encrypt(&Secret::new(input.password.clone())) {
                Ok(encrypted) => Some(encrypted),
                Err(SecretCipherError::MissingKey) => {
                    return Err(gluon::AppError::Validation(vec![field_error(
                        "auth.password",
                        "Password saving is unavailable without SECRET_KEY_BASE.",
                    )]));
                }
                Err(error) => return Err(gluon::AppError::Internal(Box::new(error))),
            }
        } else {
            previous_secret
        };
        ConnectionProfileAuth::Password { encrypted_secret }
    };

    ConnectionProfile::new(
        id,
        owner,
        input.name.clone(),
        fields.engine,
        input.host.clone(),
        fields.port,
        input.database.clone(),
        input.username.clone(),
        auth,
        fields.ssl_mode,
        nonempty(&input.ssl_root_cert_pem),
        input.read_only.is_some(),
        fields.statement_timeout_ms,
        fields.color,
    )
}

#[must_use]
pub fn saved_secret(profile: &ConnectionProfile) -> Option<EncryptedSecret> {
    match &profile.auth {
        ConnectionProfileAuth::Password { encrypted_secret } => encrypted_secret.clone(),
        ConnectionProfileAuth::DsqlIam { .. } => None,
    }
}

fn parse_engine(value: &str) -> Option<Engine> {
    match value {
        "postgres" => Some(Engine::Postgres),
        "aurora_dsql" => Some(Engine::AuroraDsql),
        "cockroach_db" => Some(Engine::CockroachDb),
        "yugabyte_db" => Some(Engine::YugabyteDb),
        _ => None,
    }
}

fn parse_ssl_mode(value: &str) -> Option<SslMode> {
    match value {
        "disable" => Some(SslMode::Disable),
        "prefer" => Some(SslMode::Prefer),
        "require" => Some(SslMode::Require),
        "verify-ca" => Some(SslMode::VerifyCa),
        "verify-full" => Some(SslMode::VerifyFull),
        _ => None,
    }
}

fn engine_name(engine: Engine) -> &'static str {
    match engine {
        Engine::Postgres => "postgres",
        Engine::AuroraDsql => "aurora_dsql",
        Engine::CockroachDb => "cockroach_db",
        Engine::YugabyteDb => "yugabyte_db",
    }
}

fn ssl_mode_name(mode: SslMode) -> &'static str {
    match mode {
        SslMode::Disable => "disable",
        SslMode::Prefer => "prefer",
        SslMode::Require => "require",
        SslMode::VerifyCa => "verify-ca",
        SslMode::VerifyFull => "verify-full",
    }
}

fn field_error(field: &str, message: &str) -> gluon::FieldError {
    gluon::FieldError {
        field: field.to_owned(),
        message: message.to_owned(),
    }
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

fn nonempty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

#[must_use]
pub fn secret_cipher_available() -> bool {
    std::env::var("SECRET_KEY_BASE").is_ok_and(|value| !value.is_empty())
}

#[must_use]
pub fn validation_response(props: ConnectionEditorProps) -> Response {
    let mut response = View::new(props).into_response();
    *response.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
    response
}
