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
    saved_query::{SavedQuery, SavedQueryId, SavedQueryRepository},
};
use pgpaint::web::{encode_url_component, redirect_response, Owner, Shell, ShellCtx};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Deserialize)]
pub struct SavedSearchQuery {
    search: Option<String>,
}

#[derive(Deserialize)]
pub struct SavedAction {
    operation: String,
    saved_id: Option<String>,
    profile_id: Option<String>,
    search: Option<String>,
    name: Option<String>,
    sql: Option<String>,
}

#[derive(Serialize)]
pub struct ProfileOption {
    id: String,
    name: String,
}

#[derive(Serialize)]
pub struct SavedEntry {
    id: String,
    name: String,
    profile_name: String,
    sql_preview: String,
    edit_href: String,
    open_href: Option<String>,
}

#[derive(Serialize)]
pub struct SavedProps {
    #[serde(flatten)]
    shell: Shell,
    profiles: Vec<ProfileOption>,
    entries: Vec<SavedEntry>,
    has_entries: bool,
    has_profiles: bool,
    search: String,
}

pub async fn get(
    Owner(owner): Owner,
    ShellCtx(mut shell): ShellCtx,
    Query(query): Query<SavedSearchQuery>,
    Inject(saved_repository): Inject<dyn SavedQueryRepository>,
    Inject(profile_repository): Inject<dyn ConnectionProfileRepository>,
) -> gluon::Result<Response> {
    let owner = owner.0;
    let profiles = profile_repository.list(Some(owner.as_str()), u32::MAX).await?;
    let search = query.search.unwrap_or_default();
    let search_filter = (!search.trim().is_empty()).then_some(search.trim());
    let saved = saved_repository
        .list_search(Some(owner.as_str()), None, u32::MAX, search_filter)
        .await?;
    let profile_names: HashMap<_, _> = profiles
        .iter()
        .map(|profile| (profile.id.0, profile.name.clone()))
        .collect();
    let entries = saved
        .into_iter()
        .map(|query| {
            let profile_name = query
                .profile_id
                .as_ref()
                .and_then(|id| profile_names.get(&id.0))
                .cloned();
            let open_href = query.profile_id.as_ref().and_then(|id| {
                profile_names
                    .get(&id.0)
                    .map(|_| format!("/c/{}/query?saved={}", id.0, query.id.0))
            });
            let profile_name = profile_name.unwrap_or_else(|| {
                if query.profile_id.is_some() {
                    "Unavailable profile".to_owned()
                } else {
                    "No default profile".to_owned()
                }
            });
            SavedEntry {
                id: query.id.0.to_string(),
                name: query.name,
                profile_name,
                sql_preview: query.sql.lines().take(3).collect::<Vec<_>>().join("\n"),
                edit_href: format!("/saved/{}", query.id.0),
                open_href,
            }
        })
        .collect::<Vec<_>>();
    let has_entries = !entries.is_empty();
    let profiles = profiles
        .into_iter()
        .map(|profile| ProfileOption {
            id: profile.id.0.to_string(),
            name: profile.name,
        })
        .collect::<Vec<_>>();
    let has_profiles = !profiles.is_empty();
    shell.title = "Saved queries · pgpaint".to_owned();
    Ok(View::new(SavedProps {
        shell,
        profiles,
        entries,
        has_entries,
        has_profiles,
        search,
    })
    .into_response())
}

pub async fn post(
    headers: HeaderMap,
    Owner(owner): Owner,
    Inject(saved_repository): Inject<dyn SavedQueryRepository>,
    Inject(profile_repository): Inject<dyn ConnectionProfileRepository>,
    Form(action): Form<SavedAction>,
) -> gluon::Result<Response> {
    let owner = owner.0;
    match action.operation.as_str() {
        "create" => {
            let name = required_text(action.name.as_deref(), "name", "Name")?;
            let name = name.trim().to_owned();
            let sql = required_text(action.sql.as_deref(), "sql", "SQL")?;
            let profile_id = resolve_profile(
                action.profile_id.as_deref(),
                Some(owner.as_str()),
                &*profile_repository,
            )
            .await?;
            let now = OffsetDateTime::now_utc();
            saved_repository
                .save(&SavedQuery {
                    id: SavedQueryId(uuid::Uuid::new_v4()),
                    owner: Some(owner),
                    profile_id,
                    name,
                    sql,
                    created_at: now,
                    updated_at: now,
                })
                .await?;
        }
        "delete" => {
            let id = saved_id(action.saved_id.as_deref())?;
            saved_repository.delete(Some(owner.as_str()), &id).await?;
        }
        "open" => {
            let id = saved_id(action.saved_id.as_deref())?;
            if saved_repository.find(Some(owner.as_str()), &id).await?.is_none() {
                return Err(gluon::AppError::NotFound);
            }
            let profile_id = resolve_profile(
                action.profile_id.as_deref(),
                Some(owner.as_str()),
                &*profile_repository,
            )
            .await?
            .ok_or(gluon::AppError::NotFound)?;
            let href = format!("/c/{}/query?saved={}", profile_id.0, id.0);
            return redirect_response(href, &headers);
        }
        _ => return Err(gluon::AppError::NotFound),
    }
    let href = saved_href(action.search.as_deref().unwrap_or_default());
    redirect_response(href, &headers)
}

fn required_text(value: Option<&str>, field: &str, label: &str) -> gluon::Result<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            gluon::AppError::Validation(vec![gluon::FieldError {
                field: field.to_owned(),
                message: format!("{label} is required."),
            }])
        })
}

fn saved_id(value: Option<&str>) -> gluon::Result<SavedQueryId> {
    value
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .map(SavedQueryId)
        .ok_or(gluon::AppError::NotFound)
}

async fn resolve_profile(
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

fn saved_href(search: &str) -> String {
    if search.is_empty() {
        "/saved".to_owned()
    } else {
        format!("/saved?search={}", encode_url_component(search))
    }
}
