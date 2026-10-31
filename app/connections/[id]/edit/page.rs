use axum::{extract::Path, response::{IntoResponse, Response}, Form};
use gluon::prelude::*;
use pgpaint::{
    domain::connection_profile::{ConnectionProfileId, ConnectionProfileRepository, SecretCipher},
    usecases::close_connection::CloseConnection,
    web::{
        connection_profiles::{
            build_profile, saved_secret, secret_cipher_available, validation_response,
            ConnectionEditorProps, ConnectionProfileFieldErrors, ConnectionProfileInput,
            ConnectionProfileValues,
        },
        manual_disconnect_key, Owner, Shell, ShellCtx,
    },
};

fn parse_id(id: &str) -> gluon::Result<ConnectionProfileId> {
    uuid::Uuid::parse_str(id)
        .map(ConnectionProfileId)
        .map_err(|_| gluon::AppError::NotFound)
}

fn editor_props(
    shell: Shell,
    id: &str,
    values: ConnectionProfileValues,
    errors: ConnectionProfileFieldErrors,
) -> ConnectionEditorProps {
    ConnectionEditorProps {
        shell,
        action: format!("/connections/{id}/edit"),
        is_edit: true,
        has_secret_cipher: secret_cipher_available(),
        values,
        errors,
    }
}

pub async fn get(
    Path(id): Path<String>,
    Owner(owner): Owner,
    Inject(profiles): Inject<dyn ConnectionProfileRepository>,
    ShellCtx(mut shell): ShellCtx,
) -> gluon::Result<View<ConnectionEditorProps>> {
    let profile_id = parse_id(&id)?;
    let profile = profiles
        .find(Some(&owner.0), &profile_id)
        .await?
        .ok_or(gluon::AppError::NotFound)?;
    shell.title = format!("Edit {} · pgpaint", profile.name);
    Ok(View::new(editor_props(
        shell,
        &id,
        ConnectionProfileValues::from_profile(&profile, &profile.id, false),
        ConnectionProfileFieldErrors::default(),
    )))
}

pub async fn post(
    Path(id): Path<String>,
    Owner(owner): Owner,
    Inject(profiles): Inject<dyn ConnectionProfileRepository>,
    Inject(cipher): Inject<dyn SecretCipher>,
    Inject(close_connection): Inject<dyn CloseConnection>,
    ShellCtx(mut shell): ShellCtx,
    session: Session,
    Form(input): Form<ConnectionProfileInput>,
) -> gluon::Result<Response> {
    let profile_id = parse_id(&id)?;
    let current = profiles
        .find(Some(&owner.0), &profile_id)
        .await?
        .ok_or(gluon::AppError::NotFound)?;
    if input.action.as_deref() == Some("delete") {
        profiles.delete(Some(&owner.0), &profile_id).await?;
        close_connection.execute(owner, profile_id.clone()).await;
        session
            .remove::<bool>(&manual_disconnect_key(&profile_id))
            .await
            .map_err(|error| gluon::AppError::Internal(Box::new(error)))?;
        Flash::set(&session, "notice", format!("Connection profile {} deleted.", current.name))
            .await
            .map_err(|error| gluon::AppError::Internal(Box::new(error)))?;
        return Ok(Redirect::to("/").into_response());
    }

    shell.title = format!("Edit {} · pgpaint", current.name);
    let previous_secret = saved_secret(&current);
    let has_saved_password = previous_secret.is_some();
    let values = ConnectionProfileValues::from_input(&input, has_saved_password);
    let profile = match build_profile(
        Some(owner.0.clone()),
        profile_id.clone(),
        &input,
        previous_secret,
        &*cipher,
    ) {
        Ok(profile) => profile,
        Err(gluon::AppError::Validation(fields)) => {
            return Ok(validation_response(editor_props(
                shell,
                &id,
                values,
                ConnectionProfileFieldErrors::from_fields(fields),
            )));
        }
        Err(error) => return Err(error),
    };
    match profiles.save(&profile).await {
        Ok(()) => {}
        Err(gluon::AppError::Validation(fields)) => {
            return Ok(validation_response(editor_props(
                shell,
                &id,
                values,
                ConnectionProfileFieldErrors::from_fields(fields),
            )));
        }
        Err(error) => return Err(error),
    }
    Flash::set(&session, "notice", format!("Connection profile {} saved.", profile.name))
        .await
        .map_err(|error| gluon::AppError::Internal(Box::new(error)))?;
    Ok(Redirect::to("/").into_response())
}
