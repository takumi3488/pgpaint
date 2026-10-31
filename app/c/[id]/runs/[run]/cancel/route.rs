use std::collections::HashMap;

use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use gluon::prelude::*;
use pgpaint::domain::{database::{DatabaseGateway, QueryId}, query_run::QueryRunStore};
use pgpaint::web::{Owner, Workspace};

pub async fn post(
    Owner(owner): Owner,
    workspace: Workspace,
    Path(path_params): Path<HashMap<String, String>>,
    Inject(database): Inject<dyn DatabaseGateway>,
    Inject(run_store): Inject<dyn QueryRunStore>,
) -> gluon::Result<Response> {
    let run_id = path_params
        .get("run")
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .map(QueryId)
        .ok_or(gluon::AppError::NotFound)?;
    if !run_store.request_cancel(&owner, &workspace.profile.id, run_id) {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    match database.cancel(&workspace.key, run_id).await {
        Ok(true) => {}
        Ok(false) => run_store.clear_cancel_request(&owner, &workspace.profile.id, run_id),
        Err(error) => {
            run_store.clear_cancel_request(&owner, &workspace.profile.id, run_id);
            return Err(error.into());
        }
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}
