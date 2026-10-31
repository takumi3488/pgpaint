use axum::{
    extract::Query,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Form,
};
use gluon::prelude::*;
use pgpaint::domain::{
    query_history::{QueryHistory, QueryHistoryId, QueryHistoryRepository, QueryHistoryStatus},
};
use pgpaint::web::{history_href, page_window, redirect_response, Owner, Shell, ShellCtx, Workspace};
use serde::{Deserialize, Serialize};

const PAGE_SIZE: u32 = 20;

#[derive(Deserialize)]
pub struct HistoryQuery {
    search: Option<String>,
    status: Option<String>,
    page: Option<u64>,
}

#[derive(Deserialize)]
pub struct HistoryAction {
    operation: String,
    history_id: Option<String>,
    search: Option<String>,
    status: Option<String>,
    page: Option<u64>,
}

#[derive(Serialize)]
pub struct HistoryEntry {
    id: String,
    profile_name: String,
    executed_at: String,
    elapsed: String,
    rows: String,
    status_label: String,
    sql_preview: String,
    sql: String,
    open_href: String,
}

#[derive(Serialize)]
pub struct HistoryProps {
    #[serde(flatten)]
    shell: Shell,
    #[serde(flatten)]
    workspace: Workspace,
    entries: Vec<HistoryEntry>,
    has_entries: bool,
    action_url: String,
    profile_id: String,
    search: String,
    status: String,
    status_all_selected: bool,
    status_succeeded_selected: bool,
    status_failed_selected: bool,
    status_canceled_selected: bool,
    page: u64,
    total_pages: u64,
    previous_page: Option<u64>,
    next_page: Option<u64>,
}

pub async fn get(
    Owner(owner): Owner,
    workspace: Workspace,
    ShellCtx(mut shell): ShellCtx,
    Query(query): Query<HistoryQuery>,
    Inject(history): Inject<dyn QueryHistoryRepository>,
) -> gluon::Result<Response> {
    let status = parse_status(query.status.as_deref())?;
    let status_value = query.status.unwrap_or_default();
    let status_all_selected = status.is_none();
    let status_succeeded_selected = status == Some(QueryHistoryStatus::Succeeded);
    let status_failed_selected = status == Some(QueryHistoryStatus::Failed);
    let status_canceled_selected = status == Some(QueryHistoryStatus::Canceled);
    let search = query.search.unwrap_or_default();
    let search_filter = (!search.trim().is_empty()).then_some(search.trim());
    let profile_id = &workspace.profile.id;
    let total_count = history
        .count(
            Some(owner.0.as_str()),
            Some(profile_id),
            search_filter,
            status,
        )
        .await?;
    let window = page_window(total_count, u64::from(PAGE_SIZE), query.page);
    let entries = history
        .list_filtered(
            Some(owner.0.as_str()),
            Some(profile_id),
            window.offset,
            PAGE_SIZE,
            search_filter,
            status,
        )
        .await?
        .into_iter()
        .map(|entry| history_entry(entry, &workspace.profile_name, workspace.id()))
        .collect::<Vec<_>>();
    let has_entries = !entries.is_empty();
    let profile_id_value = profile_id.0.to_string();
    let action_url = format!("/c/{profile_id_value}/history");
    shell.title = format!("History · {} · pgpaint", workspace.profile_name);
    Ok(View::new(HistoryProps {
        shell,
        workspace,
        entries,
        has_entries,
        action_url,
        profile_id: profile_id_value,
        search,
        status: status_value,
        status_all_selected,
        status_succeeded_selected,
        status_failed_selected,
        status_canceled_selected,
        page: window.page,
        total_pages: window.total_pages,
        previous_page: window.previous_page,
        next_page: window.next_page,
    })
    .into_response())
}

pub async fn post(
    headers: HeaderMap,
    Owner(owner): Owner,
    workspace: Workspace,
    Inject(history): Inject<dyn QueryHistoryRepository>,
    Form(action): Form<HistoryAction>,
) -> gluon::Result<Response> {
    let owner = owner.0;
    let profile_id = &workspace.profile.id;
    match action.operation.as_str() {
        "delete" => {
            let history_id = action
                .history_id
                .as_deref()
                .and_then(|value| uuid::Uuid::parse_str(value).ok())
                .map(QueryHistoryId)
                .ok_or(gluon::AppError::NotFound)?;
            history
                .delete(Some(owner.as_str()), Some(profile_id), &history_id)
                .await?;
        }
        "clear-profile" => history.prune(Some(owner.as_str()), profile_id, 0).await?,
        _ => return Err(gluon::AppError::NotFound),
    }
    let href = history_href(
        &format!("/c/{}/history", workspace.id()),
        None,
        action.search.as_deref().unwrap_or_default(),
        action.status.as_deref().unwrap_or_default(),
        action.page.unwrap_or(1),
    );
    redirect_response(href, &headers)
}

fn parse_status(value: Option<&str>) -> gluon::Result<Option<QueryHistoryStatus>> {
    match value.filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some("succeeded") => Ok(Some(QueryHistoryStatus::Succeeded)),
        Some("failed") => Ok(Some(QueryHistoryStatus::Failed)),
        Some("canceled") => Ok(Some(QueryHistoryStatus::Canceled)),
        Some(_) => Err(gluon::AppError::Validation(vec![gluon::FieldError {
            field: "status".to_owned(),
            message: "Choose a valid query status.".to_owned(),
        }])),
    }
}

fn history_entry(entry: QueryHistory, profile_name: &str, profile_id: &str) -> HistoryEntry {
    let status_label = match entry.status {
        QueryHistoryStatus::Succeeded => "Succeeded",
        QueryHistoryStatus::Failed => "Failed",
        QueryHistoryStatus::Canceled => "Canceled",
    };
    HistoryEntry {
        id: entry.id.0.to_string(),
        profile_name: profile_name.to_owned(),
        executed_at: entry.executed_at.to_string(),
        elapsed: format!("{} ms", entry.elapsed_ms),
        rows: entry
            .rows
            .or(entry.rows_affected)
            .map_or_else(|| "—".to_owned(), |rows| rows.to_string()),
        status_label: status_label.to_owned(),
        sql_preview: entry.sql.lines().take(3).collect::<Vec<_>>().join("\n"),
        sql: entry.sql,
        open_href: format!("/c/{profile_id}/query?history={}", entry.id.0),
    }
}

