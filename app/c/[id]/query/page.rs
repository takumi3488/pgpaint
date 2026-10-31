use std::time::Duration;

use axum::{
    Form,
    extract::Query,
    response::{IntoResponse, Response},
};
use gluon::prelude::*;
use pgpaint::domain::database::{
    DatabaseGateway, LintIssue, LintSeverity, QueryId, QueryRequest, QueryResult, lint,
    split_statements,
};
use pgpaint::domain::query_history::{
    QueryHistory, QueryHistoryId, QueryHistoryRepository, QueryHistoryStatus,
};
use pgpaint::domain::query_run::{QueryRun, QueryRunOutcome, QueryRunStatus, QueryRunStore};
use pgpaint::domain::saved_query::{SavedQuery, SavedQueryId, SavedQueryRepository};
use pgpaint::web::{Owner, Shell, ShellCtx, Workspace};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

const MAX_ROWS: usize = 10_000;
const HISTORY_KEEP_NEWEST: u32 = 500;

#[derive(Deserialize, Default)]
pub struct EditorQuery {
    history: Option<String>,
    saved: Option<String>,
}

#[derive(Deserialize)]
pub struct QueryForm {
    sql: String,
    action: Option<String>,
    name: Option<String>,
    force: Option<bool>,
}

#[derive(Serialize)]
pub struct QueryProps {
    #[serde(flatten)]
    shell: Shell,
    #[serde(flatten)]
    workspace: Workspace,
    sql: String,
    completion_url: String,
    query_url: String,
    explain_url: String,
    can_explain_analyze: bool,
    lint_issues: Vec<IssueView>,
    has_lint_issues: bool,
    force_available: bool,
    run: Option<RunDisplay>,
}

#[derive(Serialize, Clone)]
pub struct IssueView {
    pub severity: &'static str,
    pub message: String,
}

#[derive(Serialize, Clone)]
pub struct RunDisplay {
    id: String,
    status: &'static str,
    status_label: &'static str,
    is_running: bool,
    events_url: String,
    cancel_url: String,
    result_url: String,
    elapsed_ms: u64,
    warnings: Vec<IssueView>,
    error: Option<String>,
    selected_result: Option<()>,
    statements: Vec<()>,
    schema_event_url: Option<String>,
}


fn issue_views(issues: &[LintIssue]) -> Vec<IssueView> {
    issues
        .iter()
        .map(|issue| IssueView {
            severity: match issue.severity {
                LintSeverity::Error => "Error",
                LintSeverity::Warning => "Warning",
            },
            message: issue.message.clone(),
        })
        .collect()
}
fn running_display(id: QueryId, profile_id: &str, warnings: &[LintIssue]) -> RunDisplay {
    let run_id = id.0.to_string();
    let run_url = format!("/c/{profile_id}/runs/{run_id}");
    RunDisplay {
        id: run_id,
        status: "running",
        status_label: "Running",
        is_running: true,
        events_url: format!("{run_url}/events"),
        cancel_url: format!("{run_url}/cancel"),
        result_url: format!("{run_url}?statement=0&page=1"),
        elapsed_ms: 0,
        warnings: issue_views(warnings),
        error: None,
        selected_result: None,
        statements: Vec::new(),
        schema_event_url: None,
    }
}

fn empty_query_issue() -> LintIssue {
    LintIssue {
        severity: LintSeverity::Error,
        rule: "empty-query",
        message: "Enter a SQL statement to run.".to_owned(),
        statement_index: 0,
        offset: 0,
    }
}

pub async fn get(
    Owner(owner): Owner,
    workspace: Workspace,
    ShellCtx(mut shell): ShellCtx,
    Query(params): Query<EditorQuery>,
    Inject(history): Inject<dyn QueryHistoryRepository>,
    Inject(saved): Inject<dyn SavedQueryRepository>,
) -> gluon::Result<Response> {
    if !workspace.is_connected() {
        return Err(gluon::AppError::Conflict("Connect to this database before opening the SQL editor.".to_owned()));
    }
    if params.history.is_some() && params.saved.is_some() {
        return Err(gluon::AppError::BadRequest("Choose either a history entry or a saved query.".to_owned()));
    }

    let sql = if let Some(id) = params.history {
        let id = uuid::Uuid::parse_str(&id)
            .map(QueryHistoryId)
            .map_err(|_| gluon::AppError::NotFound)?;
        history
            .find(Some(owner.0.as_str()), &workspace.profile.id, &id)
            .await?
            .ok_or(gluon::AppError::NotFound)?
            .sql
    } else if let Some(id) = params.saved {
        let id = uuid::Uuid::parse_str(&id)
            .map(SavedQueryId)
            .map_err(|_| gluon::AppError::NotFound)?;
        let query = saved
            .find(Some(owner.0.as_str()), &id)
            .await?
            .ok_or(gluon::AppError::NotFound)?;
        if query
            .profile_id
            .as_ref()
            .is_some_and(|profile_id| profile_id != &workspace.profile.id)
        {
            return Err(gluon::AppError::NotFound);
        }
        query.sql
    } else {
        String::new()
    };

    shell.title = format!("Query · {} · pgpaint", workspace.profile_name);
    let can_explain_analyze = workspace
        .server_info
        .as_ref()
        .is_some_and(|info| info.engine.capabilities().explain_analyze);
    let id = workspace.id().to_owned();
    Ok(View::new(QueryProps {
        shell,
        workspace,
        sql,
        completion_url: format!("/c/{id}/completion"),
        query_url: format!("/c/{id}/query"),
        explain_url: format!("/c/{id}/explain"),
        can_explain_analyze,
        lint_issues: vec![],
        has_lint_issues: false,
        force_available: false,
        run: None,
    })
    .into_response())
}

#[allow(clippy::too_many_lines)]
pub async fn post(
    Owner(owner): Owner,
    workspace: Workspace,
    ShellCtx(mut shell): ShellCtx,
    session: Session,
    Inject(database): Inject<dyn DatabaseGateway>,
    Inject(run_store): Inject<dyn QueryRunStore>,
    Inject(history): Inject<dyn QueryHistoryRepository>,
    Inject(saved): Inject<dyn SavedQueryRepository>,
    Form(form): Form<QueryForm>,
) -> gluon::Result<Response> {
    if !workspace.is_connected() {
        return Err(gluon::AppError::Conflict("Connect to this database before running SQL.".to_owned()));
    }
    if form.action.as_deref() == Some("save") {
        let name = form.name.as_deref().unwrap_or_default().trim();
        if name.is_empty() || name.chars().count() > 120 || form.sql.trim().is_empty() {
            return Err(gluon::AppError::Validation(vec![gluon::FieldError {
                field: "name".to_owned(),
                message: "Enter a name of at most 120 characters and SQL to save.".to_owned(),
            }]));
        }
        let now = OffsetDateTime::now_utc();
        saved
            .save(&SavedQuery {
                id: SavedQueryId(uuid::Uuid::new_v4()),
                owner: Some(owner.0.clone()),
                profile_id: Some(workspace.profile.id.clone()),
                name: name.to_owned(),
                sql: form.sql,
                created_at: now,
                updated_at: now,
            })
            .await?;
        Flash::set(&session, "notice", "Saved query.")
            .await
            .map_err(|error| gluon::AppError::Internal(Box::new(error)))?;
        return Ok(Redirect::to(format!("/c/{}/query", workspace.id())).into_response());
    }

    let statements = split_statements(&form.sql);
    let mut issues = lint(workspace.profile.engine, &form.sql, workspace.read_only);
    if statements.is_empty() {
        issues.push(empty_query_issue());
    }
    let has_errors = issues
        .iter()
        .any(|issue| issue.severity == LintSeverity::Error);
    let has_read_only_error = issues.iter().any(|issue| issue.rule == "read-only-profile");
    let force_available = !statements.is_empty() && !has_read_only_error;
    if has_errors && (form.force != Some(true) || !force_available) {
        shell.title = format!("Query · {} · pgpaint", workspace.profile_name);
        let id = workspace.id().to_owned();
        let can_explain_analyze = workspace
            .server_info
            .as_ref()
            .is_some_and(|info| info.engine.capabilities().explain_analyze);
        return Ok(View::new(QueryProps {
            shell,
            workspace,
            sql: form.sql,
            completion_url: format!("/c/{id}/completion"),
            query_url: format!("/c/{id}/query"),
            explain_url: format!("/c/{id}/explain"),
            can_explain_analyze,
            lint_issues: issue_views(&issues),
            has_lint_issues: true,
            force_available,
            run: None,
        })
        .into_response());
    }
    let sql = form.sql;

    let id = QueryId(uuid::Uuid::new_v4());
    let profile_id = workspace.profile.id.clone();
    let run = QueryRun {
        id,
        owner: owner.clone(),
        profile_id: profile_id.clone(),
        sql: sql.clone(),
        warnings: issues,
        status: QueryRunStatus::Running,
        result: None,
        error: None,
        elapsed_ms: 0,
        cancel_requested: false,
    };
    let display = running_display(id, workspace.id(), &run.warnings);
    run_store
        .insert(run)
        .map_err(|error| gluon::AppError::Conflict(error.to_string()))?;

    let request = QueryRequest {
        id,
        sql: sql.clone(),
        max_rows: MAX_ROWS,
        timeout: workspace.profile.statement_timeout_ms.map(Duration::from_millis),
    };
    let key = workspace.key.clone();
    let owner_id = owner.clone();
    let database = std::sync::Arc::clone(&database);
    let run_store_task = std::sync::Arc::clone(&run_store);
    let history_task = std::sync::Arc::clone(&history);
    let profile_id_task = profile_id.clone();
    drop(tokio::spawn(async move {
        let outcome = match database.execute(&key, request).await {
            Ok(result) => QueryRunOutcome::Succeeded(result),
            Err(error) => {
                let message = error.to_string();
                if run_store_task
                    .get(&owner_id, &profile_id_task, id)
                    .is_some_and(|active| active.cancel_requested)
                {
                    QueryRunOutcome::Canceled(Some(message))
                } else {
                    QueryRunOutcome::Failed(message)
                }
            }
        };
        let Some(finished) = run_store_task.finish(&owner_id, &profile_id_task, id, outcome) else {
            return;
        };
        let (status, rows, rows_affected) = match (&finished.status, &finished.result) {
            (QueryRunStatus::Succeeded, result) => (
                QueryHistoryStatus::Succeeded,
                result.as_ref().map(total_rows),
                result.as_ref().map(total_rows_affected),
            ),
            (QueryRunStatus::Canceled, _) => (QueryHistoryStatus::Canceled, None, None),
            _ => (QueryHistoryStatus::Failed, None, None),
        };
        let entry = QueryHistory {
            id: QueryHistoryId(id.0),
            owner: Some(owner_id.0.clone()),
            profile_id: profile_id_task.clone(),
            sql: finished.sql.clone(),
            executed_at: OffsetDateTime::now_utc(),
            elapsed_ms: finished.elapsed_ms,
            rows,
            rows_affected,
            status,
            error_message: finished.error.clone(),
        };
        if history_task.save(&entry).await.is_ok() {
            let _ = history_task
                .prune(Some(owner_id.0.as_str()), &profile_id_task, HISTORY_KEEP_NEWEST)
                .await;
        }
    }));

    shell.title = format!("Query · {} · pgpaint", workspace.profile_name);
    let id = workspace.id().to_owned();
    let can_explain_analyze = workspace
        .server_info
        .as_ref()
        .is_some_and(|info| info.engine.capabilities().explain_analyze);
    Ok(View::new(QueryProps {
        shell,
        workspace,
        sql,
        completion_url: format!("/c/{id}/completion"),
        query_url: format!("/c/{id}/query"),
        explain_url: format!("/c/{id}/explain"),
        can_explain_analyze,
        lint_issues: vec![],
        has_lint_issues: false,
        force_available: false,
        run: Some(display),
    })
    .into_response())
}

fn total_rows(result: &QueryResult) -> u64 {
    result.statements.iter().fold(0_u64, |total, statement| {
        total.saturating_add(u64::try_from(statement.rows.len()).unwrap_or(u64::MAX))
    })
}

fn total_rows_affected(result: &QueryResult) -> u64 {
    result.statements.iter().fold(0_u64, |total, statement| {
        total.saturating_add(statement.rows_affected)
    })
}



