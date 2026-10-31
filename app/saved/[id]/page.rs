use axum::{
    extract::Path,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Form,
};
use gluon::prelude::*;
use pgpaint::domain::{
    connection_profile::{ConnectionProfileId, ConnectionProfileRepository},
    saved_query::{SavedQueryId, SavedQueryRepository},
};
use pgpaint::web::{redirect_response, Owner, Shell, ShellCtx};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Deserialize)]
pub struct SavedForm {
    operation: String,
    name: Option<String>,
    sql: Option<String>,
    profile_id: Option<String>,
}

#[derive(Serialize)]
pub struct ProfileOption {
    id: String,
    name: String,
    selected: bool,
}

#[derive(Serialize)]
pub struct SavedEditProps {
    #[serde(flatten)]
    shell: Shell,
    id: String,
    name: String,
    sql: String,
    no_profile_selected: bool,
    profiles: Vec<ProfileOption>,
    has_profiles: bool,
    open_href: Option<String>,
}

pub async fn get(
    Path(raw_id): Path<String>,
    Owner(owner): Owner,
    ShellCtx(mut shell): ShellCtx,
    Inject(saved_repository): Inject<dyn SavedQueryRepository>,
    Inject(profile_repository): Inject<dyn ConnectionProfileRepository>,
) -> gluon::Result<Response> {
    let owner = owner.0;
    let id = parse_id(&raw_id)?;
    let saved = saved_repository
        .find(Some(owner.as_str()), &id)
        .await?
        .ok_or(gluon::AppError::NotFound)?;
    let profiles = profile_repository.list(Some(owner.as_str()), u32::MAX).await?;
    let has_profiles = !profiles.is_empty();
    let current_profile = saved.profile_id.as_ref().filter(|selected| {
        profiles
            .iter()
            .any(|profile| profile.id.0 == selected.0)
    });
    let open_href = current_profile.map(|profile| {
        format!("/c/{}/query?saved={}", profile.0, saved.id.0)
    });
    let profile_options = profiles
        .into_iter()
        .map(|profile| ProfileOption {
            id: profile.id.0.to_string(),
            name: profile.name,
            selected: current_profile.is_some_and(|selected| selected == &profile.id),
        })
        .collect();
    shell.title = format!("Edit {} · Saved queries · pgpaint", saved.name);
    Ok(View::new(SavedEditProps {
        shell,
        id: saved.id.0.to_string(),
        name: saved.name,
        sql: saved.sql,
        has_profiles,
        no_profile_selected: current_profile.is_none(),
        profiles: profile_options,
        open_href,
    })
    .into_response())
}

pub async fn post(
    Path(raw_id): Path<String>,
    headers: HeaderMap,
    Owner(owner): Owner,
    Inject(saved_repository): Inject<dyn SavedQueryRepository>,
    Inject(profile_repository): Inject<dyn ConnectionProfileRepository>,
    Form(form): Form<SavedForm>,
) -> gluon::Result<Response> {
    let owner = owner.0;
    let id = parse_id(&raw_id)?;
    let mut saved = saved_repository
        .find(Some(owner.as_str()), &id)
        .await?
        .ok_or(gluon::AppError::NotFound)?;
    match form.operation.as_str() {
        "save" => {
            saved.name = required_text(form.name.as_deref(), "name", "Name")?.trim().to_owned();
            saved.sql = required_text(form.sql.as_deref(), "sql", "SQL")?;
            saved.profile_id = resolve_profile(
                form.profile_id.as_deref(),
                Some(owner.as_str()),
                &*profile_repository,
            )
            .await?;
            saved.updated_at = OffsetDateTime::now_utc();
            saved_repository.save(&saved).await?;
            redirect_response(format!("/saved/{}", saved.id.0), &headers)
        }
        "open" => {
            let profile_id = resolve_profile(
                form.profile_id.as_deref(),
                Some(owner.as_str()),
                &*profile_repository,
            )
            .await?
            .ok_or(gluon::AppError::NotFound)?;
            redirect_response(
                format!("/c/{}/query?saved={}", profile_id.0, id.0),
                &headers,
            )
        }
        "delete" => {
            saved_repository.delete(Some(owner.as_str()), &id).await?;
            redirect_response("/saved".to_owned(), &headers)
        }
        _ => Err(gluon::AppError::NotFound),
    }
}

fn parse_id(value: &str) -> gluon::Result<SavedQueryId> {
    uuid::Uuid::parse_str(value)
        .map(SavedQueryId)
        .map_err(|_| gluon::AppError::NotFound)
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
