use axum::{response::{IntoResponse, Response}, Form};
use gluon::prelude::*;
use pgpaint::usecases::open_connection::{OpenConnection, OpenConnectionError};
use pgpaint::web::{manual_disconnect_key, Owner, Shell, ShellCtx, Workspace};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ConnectForm {
    password: Option<String>,
}

#[derive(Serialize)]
pub struct ConnectProps {
    #[serde(flatten)]
    shell: Shell,
    #[serde(flatten)]
    workspace: Workspace,
    form_error: Option<String>,
}

pub async fn get(
    workspace: Workspace,
    ShellCtx(mut shell): ShellCtx,
) -> gluon::Result<Response> {
    if workspace.is_connected() {
        return Ok(Redirect::to(format!("/c/{}/query", workspace.id())).into_response());
    }
    shell.title = format!("Connect {} · pgpaint", workspace.profile_name);
    Ok(View::new(ConnectProps {
        shell,
        workspace,
        form_error: None,
    })
    .into_response())
}

pub async fn post(
    mut workspace: Workspace,
    ShellCtx(mut shell): ShellCtx,
    session: Session,
    Owner(owner): Owner,
    Inject(open_connection): Inject<dyn OpenConnection>,
    Form(form): Form<ConnectForm>,
) -> gluon::Result<Response> {
    if workspace.is_connected() {
        return Ok(Redirect::to(format!("/c/{}/query", workspace.id())).into_response());
    }

    match open_connection
        .execute(
            owner,
            workspace.profile.id.clone(),
            form.password,
        )
        .await
    {
        Ok(_) => {
            session
                .remove::<bool>(&manual_disconnect_key(&workspace.profile.id))
                .await
                .map_err(|error| gluon::AppError::Internal(Box::new(error)))?;
            Flash::set(&session, "notice", format!("Connected to {}.", workspace.profile_name))
                .await
                .map_err(|error| gluon::AppError::Internal(Box::new(error)))?;
            Ok(Redirect::to(format!("/c/{}/query", workspace.id())).into_response())
        }
        Err(OpenConnectionError::Database(error)) => {
            workspace.connect_error = Some(error);
            shell.title = format!("Connect {} · pgpaint", workspace.profile_name);
            Ok(View::new(ConnectProps {
                shell,
                workspace,
                form_error: None,
            })
            .into_response())
        }
        Err(OpenConnectionError::Application(error)) => match error {
            gluon::AppError::Validation(fields) => {
                let message = fields
                    .into_iter()
                    .map(|field| field.message)
                    .collect::<Vec<_>>()
                    .join(" ");
                shell.title = format!("Connect {} · pgpaint", workspace.profile_name);
                Ok(View::new(ConnectProps {
                    shell,
                    workspace,
                    form_error: Some(message),
                })
                .into_response())
            }
            gluon::AppError::BadRequest(message) | gluon::AppError::Conflict(message) => {
                shell.title = format!("Connect {} · pgpaint", workspace.profile_name);
                Ok(View::new(ConnectProps {
                    shell,
                    workspace,
                    form_error: Some(message),
                })
                .into_response())
            }
            other => Err(other),
        },
    }
}
