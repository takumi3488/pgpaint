pub mod connection_profiles;

use std::{collections::HashMap, io, sync::Arc};

use axum::extract::{FromRef, FromRequestParts, OptionalFromRequestParts, Path};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts};
use axum::response::{IntoResponse, Response};
use gluon::{
    AppError, Flash, Redirect, Session,
    auth::{Auth, CurrentUser},
    middleware::CsrfToken,
};
use serde::Serialize;

use crate::domain::{
    connection_profile::{
        ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId, ConnectionProfileRepository,
    },
    database::{ConnectionKey, DatabaseGateway, DbError, Engine, OwnerId, ServerInfo},
};
use crate::usecases::open_connection::{OpenConnection, OpenConnectionError};

#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Shell {
    pub csrf_token: String,
    pub auth_enabled: bool,
    pub passkeys_enabled: bool,
    pub user: Option<String>,
    pub flash: Option<Notice>,
    pub title: String,
}

pub struct ShellCtx(pub Shell);

impl<S> FromRequestParts<S> for ShellCtx
where
    S: Send + Sync,
    Arc<gluon::Container>: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let csrf_token = parts
            .extensions
            .get::<CsrfToken>()
            .map(|token| token.as_str().to_owned())
            .ok_or_else(|| internal_error("CSRF token extension is missing"))?;
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| internal_error("session extraction failed"))?;
        let flash = Flash::take(&session)
            .await
            .map_err(|error| internal_error(error.to_string()))?;
        let notice = flash
            .get("notice")
            .map(|message| Notice {
                kind: "success".to_owned(),
                message: message.to_owned(),
            })
            .or_else(|| {
                flash.get("alert").map(|message| Notice {
                    kind: "error".to_owned(),
                    message: message.to_owned(),
                })
            });
        let container = Arc::<gluon::Container>::from_ref(state);
        let auth_enabled = container.try_resolve::<Auth>().is_some();
        let passkeys_enabled = container
            .try_resolve::<crate::auth::AuthUi>()
            .is_some_and(|features| features.passkeys);
        let user = if auth_enabled {
            <CurrentUser as OptionalFromRequestParts<S>>::from_request_parts(parts, state)
                .await
                .map_err(|error| internal_error(error.to_string()))?
                .map(|user| user.0.id.0)
        } else {
            None
        };

        Ok(Self(Shell {
            csrf_token,
            auth_enabled,
            passkeys_enabled,
            user,
            flash: notice,
            title: "pgpaint".to_owned(),
        }))
    }
}

/// Current data owner. Local single-user mode uses the stable `local` owner;
/// shared mode uses gluon's authenticated, stable user ID.
pub struct Owner(pub OwnerId);

impl<S> FromRequestParts<S> for Owner
where
    S: Send + Sync,
    Arc<gluon::Container>: FromRef<S>,
{
    type Rejection = RequestRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let container = Arc::<gluon::Container>::from_ref(state);
        if container.try_resolve::<Auth>().is_none() {
            return Ok(Self(OwnerId("local".to_owned())));
        }

        match <CurrentUser as FromRequestParts<S>>::from_request_parts(parts, state).await {
            Ok(user) => Ok(Self(OwnerId(user.0.id.0))),
            Err(error) if error.code == "unauthorized" && wants_html(parts) => {
                Err(RequestRejection(Redirect::to("/login").into_response()))
            }
            Err(error) => Err(RequestRejection(error.into_response())),
        }
    }
}
pub struct RequestRejection(Response);

impl From<AppError> for RequestRejection {
    fn from(error: AppError) -> Self {
        Self(error.into_response())
    }
}

impl IntoResponse for RequestRejection {
    fn into_response(self) -> Response {
        self.0
    }
}

fn wants_html(parts: &Parts) -> bool {
    let path = parts.uri.path();
    if matches!(
        path.rsplit('/').next(),
        Some("completion" | "graph" | "events" | "cancel" | "export" | "disconnect")
    ) {
        return false;
    }
    let accept = parts
        .headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    !accept.contains("application/json") && !accept.contains("text/event-stream")
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceTab {
    pub label: String,
    pub href: String,
    pub active: bool,
}

/// A profile-scoped workspace context. Rust consumers use `profile`, `key`,
/// and `server_info`; serialized fields are ready for the workspace layouts.
#[allow(clippy::struct_excessive_bools)] // Independent workspace capabilities are serialized into page/layout props.
#[derive(Serialize)]
pub struct Workspace {
    #[serde(skip)]
    pub profile: ConnectionProfile,
    #[serde(skip)]
    pub key: ConnectionKey,
    pub profile_id: String,
    pub profile_name: String,
    pub engine: String,
    pub engine_label: String,
    pub server_version: String,
    pub server_info: Option<ServerInfo>,
    pub connected: bool,
    pub read_only: bool,
    pub has_saved_password: bool,
    pub password_required: bool,
    pub connect_error: Option<DbError>,
    pub tree_url: String,
    pub connect_url: String,
    pub disconnect_url: String,
    pub tabs: Vec<WorkspaceTab>,
}

impl Workspace {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.profile_id
    }

    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connected
    }
}

/// Session key used to keep an explicitly disconnected profile from auto-connecting
/// again until the user chooses Connect.
#[must_use]
pub fn manual_disconnect_key(profile_id: &ConnectionProfileId) -> String {
    format!("pgpaint:manual-disconnect:{}", profile_id.0)
}

impl<S> FromRequestParts<S> for Workspace
where
    S: Send + Sync,
    Arc<gluon::Container>: FromRef<S>,
{
    type Rejection = RequestRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(path) =
            <Path<HashMap<String, String>> as FromRequestParts<S>>::from_request_parts(
                parts, state,
            )
            .await
            .map_err(|_| AppError::NotFound)?;
        let raw_id = path.get("id").ok_or(AppError::NotFound)?;
        let id = uuid::Uuid::parse_str(raw_id).map_err(|_| AppError::NotFound)?;
        let profile_id = ConnectionProfileId(id);
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| internal_error("session extraction failed"))?;
        let Owner(owner) = <Owner as FromRequestParts<S>>::from_request_parts(parts, state).await?;
        let container = Arc::<gluon::Container>::from_ref(state);
        let profiles = container
            .try_resolve::<dyn ConnectionProfileRepository>()
            .ok_or_else(|| internal_error("connection profile repository is not configured"))?;
        let profile = profiles
            .find(Some(&owner.0), &profile_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let manually_disconnected = session
            .get::<bool>(&manual_disconnect_key(&profile_id))
            .await
            .map_err(|_| internal_error("session lookup failed"))?
            .unwrap_or(false);
        let key = ConnectionKey {
            owner,
            profile: profile.id.clone(),
        };
        let gateway = container
            .try_resolve::<dyn DatabaseGateway>()
            .ok_or_else(|| internal_error("database gateway is not configured"))?;
        let open_connection = container
            .try_resolve::<dyn OpenConnection>()
            .ok_or_else(|| internal_error("open connection usecase is not configured"))?;
        let mut server_info = gateway.server_info(&key);
        let mut connect_error = None;
        let has_saved_password = matches!(
            &profile.auth,
            ConnectionProfileAuth::Password {
                encrypted_secret: Some(_)
            }
        );
        let auto_connect = !manually_disconnected
            && (has_saved_password
                || matches!(&profile.auth, ConnectionProfileAuth::DsqlIam { .. }));

        if server_info.is_none() && auto_connect {
            match open_connection
                .execute(key.owner.clone(), profile.id.clone(), None)
                .await
            {
                Ok(info) => server_info = Some(info),
                Err(OpenConnectionError::Database(error)) => connect_error = Some(error),
                Err(OpenConnectionError::Application(error)) => return Err(error.into()),
            }
        }

        let connected = server_info.is_some();
        let engine_label = profile.engine.label().to_owned();
        let path = parts.uri.path();
        let tabs = workspace_tabs(raw_id, path, profile.engine);
        let password_required = matches!(&profile.auth, ConnectionProfileAuth::Password { .. });

        Ok(Self {
            profile_id: raw_id.clone(),
            profile_name: profile.name.clone(),
            engine: engine_slug(profile.engine).to_owned(),
            engine_label,
            server_version: server_info
                .as_ref()
                .map_or_else(|| "Not connected".to_owned(), |info| info.version.clone()),
            server_info,
            connected,
            read_only: profile.read_only,
            has_saved_password,
            password_required,
            connect_error,
            tree_url: format!("/c/{raw_id}/tree"),
            connect_url: format!("/c/{raw_id}"),
            disconnect_url: format!("/c/{raw_id}/disconnect"),
            profile,
            key,
            tabs,
        })
    }
}

/// Encodes one UTF-8 URL path or query component using RFC 3986 unreserved characters.
#[must_use]
pub fn encode_url_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageWindow {
    pub page: u64,
    pub total_pages: u64,
    pub offset: u64,
    pub previous_page: Option<u64>,
    pub next_page: Option<u64>,
}

/// Computes a one-based page window for a bounded page size and optional requested page.
#[must_use]
pub fn page_window(total_count: u64, page_size: u64, requested_page: Option<u64>) -> PageWindow {
    let page_size = page_size.max(1);
    let total_pages = total_count.div_ceil(page_size).max(1);
    let page = requested_page.unwrap_or(1).clamp(1, total_pages);
    PageWindow {
        page,
        total_pages,
        offset: page.saturating_sub(1).saturating_mul(page_size),
        previous_page: (page > 1).then_some(page - 1),
        next_page: (page < total_pages).then_some(page + 1),
    }
}

/// Builds history navigation URLs with consistently encoded filter values.
#[must_use]
pub fn history_href(
    base: &str,
    profile: Option<&str>,
    search: &str,
    status: &str,
    page: u64,
) -> String {
    let mut query = Vec::with_capacity(4);
    if let Some(profile) = profile.filter(|value| !value.is_empty()) {
        query.push(format!("profile_id={}", encode_url_component(profile)));
    }
    if !search.is_empty() {
        query.push(format!("search={}", encode_url_component(search)));
    }
    if !status.is_empty() {
        query.push(format!("status={}", encode_url_component(status)));
    }
    if page > 1 {
        query.push(format!("page={page}"));
    }
    if query.is_empty() {
        base.to_owned()
    } else {
        format!("{base}?{}", query.join("&"))
    }
}

/// Redirects htmx requests with `HX-Redirect` and ordinary requests with a 303 response.
///
/// # Errors
/// Returns an internal error if the target cannot be represented as an HTTP header.
pub fn redirect_response(href: String, headers: &HeaderMap) -> gluon::Result<Response> {
    if headers
        .get("HX-Request")
        .and_then(|value| value.to_str().ok())
        == Some("true")
    {
        let location =
            HeaderValue::from_str(&href).map_err(|error| AppError::Internal(Box::new(error)))?;
        let mut response = StatusCode::OK.into_response();
        response.headers_mut().insert("HX-Redirect", location);
        Ok(response)
    } else {
        Ok(Redirect::to(href).into_response())
    }
}

fn workspace_tabs(profile_id: &str, path: &str, engine: Engine) -> Vec<WorkspaceTab> {
    let query_href = format!("/c/{profile_id}/query");
    let er_href = format!("/c/{profile_id}/er/public");
    let history_href = "/history".to_owned();
    let mut tabs = vec![
        WorkspaceTab {
            label: "Query".to_owned(),
            active: path == query_href,
            href: query_href,
        },
        WorkspaceTab {
            label: "ER diagram".to_owned(),
            active: path.starts_with(&format!("/c/{profile_id}/er/")),
            href: er_href,
        },
        WorkspaceTab {
            label: "History".to_owned(),
            active: path == history_href,
            href: history_href,
        },
    ];
    for view in engine.views() {
        let href = format!("/c/{profile_id}/views/{}", view.id);
        tabs.push(WorkspaceTab {
            label: view.title.to_owned(),
            active: path == href,
            href,
        });
    }
    tabs
}

fn engine_slug(engine: Engine) -> &'static str {
    match engine {
        Engine::Postgres => "postgres",
        Engine::AuroraDsql => "aurora_dsql",
        Engine::CockroachDb => "cockroach_db",
        Engine::YugabyteDb => "yugabyte_db",
    }
}

fn internal_error(message: impl Into<String>) -> AppError {
    AppError::Internal(Box::new(io::Error::other(message.into())))
}

#[cfg(test)]
mod tests {
    use super::{encode_url_component, history_href, page_window};

    #[test]
    fn encodes_path_and_query_components() {
        assert_eq!(
            encode_url_component("Order details/é"),
            "Order%20details%2F%C3%A9"
        );
    }

    #[test]
    fn computes_clamped_page_windows() {
        assert_eq!(
            page_window(51, 50, Some(99)),
            super::PageWindow {
                page: 2,
                total_pages: 2,
                offset: 50,
                previous_page: Some(1),
                next_page: None,
            }
        );
        assert_eq!(
            page_window(0, 50, None),
            super::PageWindow {
                page: 1,
                total_pages: 1,
                offset: 0,
                previous_page: None,
                next_page: None,
            }
        );
    }

    #[test]
    fn builds_encoded_history_filters() {
        assert_eq!(
            history_href("/history", Some("id/01"), "a b&", "failed", 2,),
            "/history?profile_id=id%2F01&search=a%20b%26&status=failed&page=2"
        );
    }
}
