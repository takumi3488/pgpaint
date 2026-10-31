use std::collections::HashMap;

use axum::{
    extract::Query,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Form,
};
use gluon::prelude::*;
use pgpaint::domain::{
    connection_profile::{ConnectionProfileId, ConnectionProfileRepository},
    query_history::{QueryHistory, QueryHistoryId, QueryHistoryRepository, QueryHistoryStatus},
};
use pgpaint::web::{history_href, page_window, redirect_response, Owner, Shell, ShellCtx};
use serde::{Deserialize, Serialize};

const PAGE_SIZE: u32 = 20;

#[derive(Deserialize)]
pub struct HistoryQuery {
    search: Option<String>,
    profile_id: Option<String>,
    status: Option<String>,
    page: Option<u64>,
}

#[derive(Deserialize)]
pub struct HistoryAction {
    operation: String,
    history_id: Option<String>,
    profile_id: Option<String>,
    search: Option<String>,
    status: Option<String>,
    page: Option<u64>,
}

#[derive(Serialize)]
pub struct HistoryProfileOption {
    id: String,
    name: String,
    selected: bool,
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
    profiles: Vec<HistoryProfileOption>,
    entries: Vec<HistoryEntry>,
    profile_id: String,
    all_profiles_selected: bool,
    has_entries: bool,
    selected_profile_name: String,
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
    ShellCtx(mut shell): ShellCtx,
    Query(query): Query<HistoryQuery>,
    Inject(history): Inject<dyn QueryHistoryRepository>,
    Inject(profile_repository): Inject<dyn ConnectionProfileRepository>,
) -> gluon::Result<Response> {
    let profiles = profile_repository
        .list(Some(owner.0.as_str()), u32::MAX)
        .await?;
    let selected_profile = profile_id(
        query.profile_id.as_deref(),
        Some(owner.0.as_str()),
        &*profile_repository,
    )
    .await?;
    let selected_profile_name = selected_profile
        .as_ref()
        .and_then(|selected| profiles.iter().find(|profile| &profile.id == selected))
        .map_or_else(String::new, |profile| profile.name.clone());
    let status = parse_status(query.status.as_deref())?;
    let status_value = query.status.unwrap_or_default();
    let status_all_selected = status.is_none();
    let status_succeeded_selected = status == Some(QueryHistoryStatus::Succeeded);
    let status_failed_selected = status == Some(QueryHistoryStatus::Failed);
    let status_canceled_selected = status == Some(QueryHistoryStatus::Canceled);
    let search = query.search.unwrap_or_default();
    let search_filter = (!search.trim().is_empty()).then_some(search.trim());
    let total_count = history
        .count(
            Some(owner.0.as_str()),
            selected_profile.as_ref(),
            search_filter,
            status,
        )
        .await?;
    let window = page_window(total_count, u64::from(PAGE_SIZE), query.page);
    let entries = history
        .list_filtered(
            Some(owner.0.as_str()),
            selected_profile.as_ref(),
            window.offset,
            PAGE_SIZE,
            search_filter,
            status,
        )
        .await?;
    let profile_names: HashMap<_, _> = profiles
        .iter()
        .map(|profile| (profile.id.0, profile.name.clone()))
        .collect();

    shell.title = "Query history · pgpaint".to_owned();
    let entries = entries
        .into_iter()
        .map(|entry| history_entry(entry, &profile_names))
        .collect::<Vec<_>>();
    let has_entries = !entries.is_empty();
    let all_profiles_selected = selected_profile.is_none();
    let profiles = profiles
        .into_iter()
        .map(|profile| HistoryProfileOption {
            id: profile.id.0.to_string(),
            name: profile.name,
            selected: selected_profile
                .as_ref()
                .is_some_and(|selected| selected == &profile.id),
        })
        .collect::<Vec<_>>();
    Ok(View::new(HistoryProps {
        shell,
        all_profiles_selected,
        profiles,
        has_entries,
        entries,
        profile_id: selected_profile.map_or_else(String::new, |profile| profile.0.to_string()),
        selected_profile_name,
        status_all_selected,
        status_succeeded_selected,
        status_failed_selected,
        status_canceled_selected,
        search,
        status: status_value,
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
    Inject(history): Inject<dyn QueryHistoryRepository>,
    Inject(profile_repository): Inject<dyn ConnectionProfileRepository>,
    Form(action): Form<HistoryAction>,
) -> gluon::Result<Response> {
    let owner = owner.0;
    let profile = profile_id(
        action.profile_id.as_deref(),
        Some(owner.as_str()),
        &*profile_repository,
    )
    .await?;
    match action.operation.as_str() {
        "delete" => {
            let history_id = action
                .history_id
                .as_deref()
                .and_then(|value| uuid::Uuid::parse_str(value).ok())
                .map(QueryHistoryId)
                .ok_or(gluon::AppError::NotFound)?;
            history
                .delete(Some(owner.as_str()), profile.as_ref(), &history_id)
                .await?;
        }
        "clear-profile" => {
            let profile = profile.ok_or(gluon::AppError::NotFound)?;
            history.prune(Some(owner.as_str()), &profile, 0).await?;
        }
        _ => return Err(gluon::AppError::NotFound),
    }
    let href = history_href(
        "/history",
        action.profile_id.as_deref(),
        action.search.as_deref().unwrap_or_default(),
        action.status.as_deref().unwrap_or_default(),
        action.page.unwrap_or(1),
    );
    redirect_response(href, &headers)
}

async fn profile_id(
    value: Option<&str>,
    owner: Option<&str>,
    repository: &dyn ConnectionProfileRepository,
) -> gluon::Result<Option<ConnectionProfileId>> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let id = uuid::Uuid::parse_str(value).map_err(|_| gluon::AppError::NotFound)?;
    let id = ConnectionProfileId(id);
    repository
        .find(owner, &id)
        .await?
        .ok_or(gluon::AppError::NotFound)?;
    Ok(Some(id))
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

fn history_entry(entry: QueryHistory, profile_names: &HashMap<uuid::Uuid, String>) -> HistoryEntry {
    let status_label = match entry.status {
        QueryHistoryStatus::Succeeded => "Succeeded",
        QueryHistoryStatus::Failed => "Failed",
        QueryHistoryStatus::Canceled => "Canceled",
    };
    let sql_preview = entry.sql.lines().take(3).collect::<Vec<_>>().join("\n");
    HistoryEntry {
        id: entry.id.0.to_string(),
        profile_name: profile_names
            .get(&entry.profile_id.0)
            .cloned()
            .unwrap_or_else(|| "Unavailable profile".to_owned()),
        executed_at: entry.executed_at.to_string(),
        elapsed: format!("{} ms", entry.elapsed_ms),
        rows: entry
            .rows
            .or(entry.rows_affected)
            .map_or_else(|| "—".to_owned(), |rows| rows.to_string()),
        status_label: status_label.to_owned(),
        sql_preview,
        sql: entry.sql,
        open_href: format!("/c/{}/query?history={}", entry.profile_id.0, entry.id.0),
    }
}

