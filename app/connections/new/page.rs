use axum::{extract::Query, response::{IntoResponse, Response}, Form};
use gluon::prelude::*;
use pgpaint::{
    domain::connection_profile::{ConnectionProfileId, ConnectionProfileRepository},
    web::{
        connection_profiles::{
            build_profile, saved_secret, secret_cipher_available, validation_response,
            ConnectionEditorProps, ConnectionProfileFieldErrors, ConnectionProfileInput,
            ConnectionProfileValues,
        },
        Owner, Shell, ShellCtx,
    },
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct NewConnectionQuery {
    duplicate: Option<String>,
}

pub async fn get(
    Query(query): Query<NewConnectionQuery>,
    Owner(owner): Owner,
    Inject(profiles): Inject<dyn ConnectionProfileRepository>,
    ShellCtx(mut shell): ShellCtx,
) -> gluon::Result<View<ConnectionEditorProps>> {
    shell.title = "New connection · pgpaint".to_owned();
    let values = if let Some(duplicate) = query.duplicate {
        let id = uuid::Uuid::parse_str(&duplicate)
            .map(ConnectionProfileId)
            .map_err(|_| gluon::AppError::NotFound)?;
        let profile = profiles
            .find(Some(&owner.0), &id)
            .await?
            .ok_or(gluon::AppError::NotFound)?;
        ConnectionProfileValues::from_profile(&profile, &profile.id, true)
    } else {
        ConnectionProfileValues::empty()
    };
    Ok(View::new(ConnectionEditorProps {
        shell,
        action: "/connections/new".to_owned(),
        is_edit: false,
        has_secret_cipher: secret_cipher_available(),
        values,
        errors: ConnectionProfileFieldErrors::default(),
    }))
}

pub async fn post(
    Owner(owner): Owner,
    Inject(profiles): Inject<dyn ConnectionProfileRepository>,
    Inject(cipher): Inject<dyn pgpaint::domain::connection_profile::SecretCipher>,
    ShellCtx(mut shell): ShellCtx,
    session: Session,
    Form(input): Form<ConnectionProfileInput>,
) -> gluon::Result<Response> {
    shell.title = "New connection · pgpaint".to_owned();
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
    let previous_secret = source_profile.as_ref().and_then(saved_secret);
    let has_saved_password = previous_secret.is_some();
    let values = ConnectionProfileValues::from_input(&input, has_saved_password);
    let profile = match build_profile(
        Some(owner.0.clone()),
        ConnectionProfileId(uuid::Uuid::new_v4()),
        &input,
        previous_secret,
        &*cipher,
    ) {
        Ok(profile) => profile,
        Err(gluon::AppError::Validation(fields)) => {
            return Ok(validation_response(ConnectionEditorProps {
                shell,
                action: "/connections/new".to_owned(),
                is_edit: false,
                has_secret_cipher: secret_cipher_available(),
                values,
                errors: ConnectionProfileFieldErrors::from_fields(fields),
            }));
        }
        Err(error) => return Err(error),
    };
    match profiles.save(&profile).await {
        Ok(()) => {}
        Err(gluon::AppError::Validation(fields)) => {
            return Ok(validation_response(ConnectionEditorProps {
                shell,
                action: "/connections/new".to_owned(),
                is_edit: false,
                has_secret_cipher: secret_cipher_available(),
                values,
                errors: ConnectionProfileFieldErrors::from_fields(fields),
            }));
        }
        Err(error) => return Err(error),
    }
    Flash::set(&session, "notice", format!("Connection profile {} saved.", profile.name))
        .await
        .map_err(|error| gluon::AppError::Internal(Box::new(error)))?;
    Ok(Redirect::to("/").into_response())
}
