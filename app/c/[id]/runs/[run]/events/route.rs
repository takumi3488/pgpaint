use std::{collections::HashMap, time::Duration};

use axum::extract::Path;
use gluon::prelude::*;
use pgpaint::domain::{database::QueryId, query_run::{QueryRunStatus, QueryRunStore}};
use pgpaint::web::{Owner, Workspace};
use serde::Serialize;

use super::super::page::run_display;

#[derive(Serialize)]
struct RunEventProps {
    run: super::super::page::RunDisplay,
}

pub async fn get(
    Owner(owner): Owner,
    workspace: Workspace,
    Path(path_params): Path<HashMap<String, String>>,
    Inject(run_store): Inject<dyn QueryRunStore>,
) -> gluon::Result<impl axum::response::IntoResponse> {
    let run_id = path_params
        .get("run")
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .map(QueryId)
        .ok_or(gluon::AppError::NotFound)?;
    run_store
        .get(&owner, &workspace.profile.id, run_id)
        .ok_or(gluon::AppError::NotFound)?;

    let owner_id = owner;
    let profile_id = workspace.profile.id;
    let profile_id_text = workspace.profile_id;
    let updates = async_stream::stream! {
        loop {
            let Some(run) = run_store.get(&owner_id, &profile_id, run_id) else {
                break;
            };
            let event = match run.status {
                QueryRunStatus::Running => {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                    continue;
                }
                QueryRunStatus::Succeeded => "done",
                QueryRunStatus::Failed => "failed",
                QueryRunStatus::Canceled => "canceled",
            };
            let props = RunEventProps {
                run: run_display(&run, &profile_id_text, 0, 1, true),
            };
            yield sse::view_event(event, View::new(props));
            break;
        }
    };
    Ok(sse::Sse::new(updates))
}
