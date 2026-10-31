use axum::extract::{Path, Query};
use gluon::{Inject, Redirect, auth::{Auth, AuthContext, AuthError}};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CallbackQuery {
    state: String,
    #[serde(default)]
    code: String,
}

pub async fn get(
    Path(provider): Path<String>,
    Query(query): Query<CallbackQuery>,
    Inject(service): Inject<Auth>,
    context: AuthContext,
) -> Result<Redirect, AuthError> {
    service
        .finish_oauth(&context, &provider, &query.state, &query.code)
        .await?;
    Ok(Redirect::to("/"))
}
