use axum::{response::{IntoResponse, Response}, Form};
use gluon::prelude::*;
use pgpaint::{
    domain::{
        connection_profile::{
            ConnectionProfileAuth, ConnectionProfileId, ConnectionProfileRepository, SecretCipher,
        },
        database::{
            DatabaseGateway, DbError, DbErrorKind, Secret, ServerInfo,
        },
    },
    web::{
        connection_profiles::{
            build_profile, saved_secret, ConnectionProfileInput,
        },
        Owner, Shell, ShellCtx,
    },
};
use serde::Serialize;

#[derive(Serialize)]
struct TestServerInfo {
    engine_label: String,
    version: String,
    database: String,
    user: String,
    mismatch_warning: Option<String>,
}

#[derive(Serialize)]
pub struct TestConnectionProps {
    #[serde(flatten)]
    shell: Shell,
    server_info: Option<TestServerInfo>,
    error: Option<DbError>,
}

fn server_info(info: ServerInfo) -> TestServerInfo {
    let mismatch_warning = info.engine_mismatch.map(|engine| {
        format!(
            "The server appears to be {}, which differs from this profile's configured engine.",
            engine.label()
        )
    });
    TestServerInfo {
        engine_label: info.engine.label().to_owned(),
        version: info.version,
        database: info.database,
        user: info.user,
        mismatch_warning,
    }
}

fn invalid_profile_error(error: gluon::AppError) -> gluon::Result<DbError> {
    match error {
        gluon::AppError::Validation(fields) => Ok(DbError::new(
            DbErrorKind::Connect,
            fields
                .into_iter()
                .map(|field| format!("{}: {}", field.field, field.message))
                .collect::<Vec<_>>()
                .join(" "),
        )),
        other => Err(other),
    }
}

pub async fn post(
    Owner(owner): Owner,
    Inject(profiles): Inject<dyn ConnectionProfileRepository>,
    Inject(cipher): Inject<dyn SecretCipher>,
    Inject(gateway): Inject<dyn DatabaseGateway>,
    ShellCtx(mut shell): ShellCtx,
    Form(mut input): Form<ConnectionProfileInput>,
) -> gluon::Result<Response> {
    shell.title = "Test connection · pgpaint".to_owned();
    let source_profile = if let Some(source_id) = input
        .source_profile_id
        .as_deref()
        .filter(|source_id| !source_id.is_empty())
    {
        let id = uuid::Uuid::parse_str(source_id)
            .map(ConnectionProfileId)
            .map_err(|_| gluon::AppError::NotFound)?;
        Some(
            profiles
                .find(Some(&owner.0), &id)
                .await?
                .ok_or(gluon::AppError::NotFound)?,
        )
    } else {
        None
    };
    let saved = source_profile.as_ref().and_then(saved_secret);
    let saved_for_test = if !input.password.is_empty() {
        Some(Secret::new(input.password.clone()))
    } else if input.engine == "aurora_dsql" {
        None
    } else if let Some(secret) = saved.as_ref() {
        match cipher.decrypt(secret) {
            Ok(secret) => Some(secret),
            Err(_) => {
                let error = DbError::new(
                    DbErrorKind::Auth,
                    "The saved password could not be decrypted. Enter it again to test this connection.",
                );
                return Ok(View::new(TestConnectionProps {
                    shell,
                    server_info: None,
                    error: Some(error),
                })
                .into_response());
            }
        }
    } else {
        Some(Secret::new(String::new()))
    };
    input.save_password = None;
    input.clear_saved_password = None;
    let profile = match build_profile(
        Some(owner.0),
        ConnectionProfileId(uuid::Uuid::new_v4()),
        &input,
        saved,
        &*cipher,
    ) {
        Ok(profile) => profile,
        Err(error) => {
            let error = invalid_profile_error(error)?;
            return Ok(View::new(TestConnectionProps {
                shell,
                server_info: None,
                error: Some(error),
            })
            .into_response());
        }
    };
    let secret = match &profile.auth {
        ConnectionProfileAuth::Password { .. } => saved_for_test,
        ConnectionProfileAuth::DsqlIam { .. } => None,
    };
    let target = match profile.connect_target(secret) {
        Ok(target) => target,
        Err(error) => {
            let error = invalid_profile_error(error)?;
            return Ok(View::new(TestConnectionProps {
                shell,
                server_info: None,
                error: Some(error),
            })
            .into_response());
        }
    };
    let (server_info, error) = match gateway.test(&target).await {
        Ok(info) => (Some(server_info(info)), None),
        Err(error) => (None, Some(error)),
    };
    Ok(View::new(TestConnectionProps {
        shell,
        server_info,
        error,
    })
    .into_response())
}
