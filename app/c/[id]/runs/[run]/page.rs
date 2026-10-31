use std::collections::HashMap;

use axum::{
    extract::{Path, Query},
    http::{HeaderName, HeaderValue},
    response::{IntoResponse, Response},
};
use gluon::prelude::*;
use pgpaint::domain::database::{
    ColumnMeta, LintIssue, LintSeverity, QueryId, QueryResult, StatementClass, StatementResult,
    classify, split_statements,
};
use pgpaint::domain::query_run::{QueryRun, QueryRunStatus, QueryRunStore};
use pgpaint::web::{page_window, Owner, Shell, ShellCtx, Workspace};
use serde::{Deserialize, Serialize};

const PAGE_SIZE: usize = 100;

#[derive(Serialize)]
pub struct RunPageProps {
    #[serde(flatten)]
    shell: Shell,
    #[serde(flatten)]
    workspace: Workspace,
    run: RunDisplay,
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
    selected_result: Option<StatementDisplay>,
    statements: Vec<StatementDisplay>,
    schema_event_url: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct StatementDisplay {
    index: usize,
    label: String,
    href: String,
    selected: bool,
    summary: String,
    truncated: bool,
    columns: Vec<ColumnDisplay>,
    has_columns: bool,
    rows: Vec<RowDisplay>,
    page: u64,
    total_pages: u64,
    previous_href: Option<String>,
    next_href: Option<String>,
    csv_href: Option<String>,
    json_href: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct ColumnDisplay {
    label: String,
    type_name: String,
}

#[derive(Serialize, Clone)]
pub struct RowDisplay {
    cells: Vec<CellDisplay>,
}

#[derive(Serialize, Clone)]
pub struct CellDisplay {
    value: String,
    is_null: bool,
}

#[derive(Deserialize, Default)]
pub struct RunQuery {
    statement: Option<usize>,
    page: Option<usize>,
    schema_event: Option<bool>,
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

pub fn run_display(
    run: &QueryRun,
    profile_id: &str,
    selected_statement: usize,
    page: usize,
    schema_event: bool,
) -> RunDisplay {
    let run_id = run.id.0.to_string();
    let run_url = format!("/c/{profile_id}/runs/{run_id}");
    let result_url = format!("{run_url}?statement={selected_statement}&page={page}");
    let statement_spans = split_statements(&run.sql);
    let schema_changed = statement_spans
        .iter()
        .any(|statement| classify(statement.text) == StatementClass::Ddl);
    let schema_event_url =
        (schema_event && schema_changed).then(|| format!("{result_url}&schema_event=true"));
    let statements = run.result.as_ref().map_or_else(Vec::new, |result| {
        result
            .statements
            .iter()
            .enumerate()
            .map(|(index, statement)| {
                statement_display(
                    index,
                    statement,
                    statement_spans.get(index).map(|span| span.text),
                    &run_url,
                    selected_statement,
                    page,
                )
            })
            .collect()
    });
    let selected_result = statements
        .iter()
        .find(|statement| statement.selected)
        .cloned();
    let status = match run.status {
        QueryRunStatus::Running => "running",
        QueryRunStatus::Succeeded => "succeeded",
        QueryRunStatus::Failed => "failed",
        QueryRunStatus::Canceled => "canceled",
    };
    let status_label = match run.status {
        QueryRunStatus::Running => "Running",
        QueryRunStatus::Succeeded => "Completed",
        QueryRunStatus::Failed => "Failed",
        QueryRunStatus::Canceled => "Canceled",
    };
    RunDisplay {
        id: run_id,
        status,
        status_label,
        is_running: run.status == QueryRunStatus::Running,
        events_url: format!("{run_url}/events"),
        cancel_url: format!("{run_url}/cancel"),
        result_url,
        elapsed_ms: run.elapsed_ms,
        warnings: issue_views(&run.warnings),
        error: run.error.clone(),
        selected_result,
        statements,
        schema_event_url,
    }
}

fn statement_display(
    index: usize,
    statement: &StatementResult,
    sql: Option<&str>,
    run_url: &str,
    selected_statement: usize,
    requested_page: usize,
) -> StatementDisplay {
    let row_count = statement.rows.len();
    let requested_page = (index == selected_statement)
        .then(|| u64::try_from(requested_page).unwrap_or(u64::MAX));
    let window = page_window(
        u64::try_from(statement.rows.len()).unwrap_or(u64::MAX),
        u64::try_from(PAGE_SIZE).unwrap_or(u64::MAX),
        requested_page,
    );
    let offset = usize::try_from(window.offset).unwrap_or(usize::MAX);
    let rows = statement
        .rows
        .iter()
        .skip(offset)
        .take(PAGE_SIZE)
        .map(|row| RowDisplay {
            cells: row
                .iter()
                .map(|cell| CellDisplay {
                    value: cell.clone().unwrap_or_default(),
                    is_null: cell.is_none(),
                })
                .collect(),
        })
        .collect();
    let href = format!("{run_url}?statement={index}&page=1");
    let selected = index == selected_statement;
    let statement_class = classify(sql.unwrap_or_default());
    let can_export = statement_class == StatementClass::Query;
    let has_columns = !statement.columns.is_empty();
    let summary = if statement_class == StatementClass::Query {
        format!("{row_count} rows returned")
    } else if has_columns && statement_class == StatementClass::Dml {
        format!("{row_count} rows returned · {} rows affected", statement.rows_affected)
    } else if has_columns {
        format!("{row_count} rows returned")
    } else {
        format!("{} rows affected", statement.rows_affected)
    };
    let export_base = format!("{run_url}/export?statement={index}");
    StatementDisplay {
        index,
        label: format!("Statement {}", index + 1),
        href,
        selected,
        summary,
        truncated: statement.truncated,
        columns: statement
            .columns
            .iter()
            .map(|column: &ColumnMeta| ColumnDisplay {
                label: column.name.clone(),
                type_name: column.type_name.clone(),
            })
            .collect(),
        has_columns,
        rows,
        page: window.page,
        total_pages: window.total_pages,
        previous_href: window
            .previous_page
            .map(|page| format!("{run_url}?statement={index}&page={page}")),
        next_href: window
            .next_page
            .map(|page| format!("{run_url}?statement={index}&page={page}")),
        csv_href: can_export.then(|| format!("{export_base}&format=csv")),
        json_href: can_export.then(|| format!("{export_base}&format=json")),
    }
}

pub fn run_schema_changed(sql: &str) -> bool {
    split_statements(sql)
        .iter()
        .any(|statement| classify(statement.text) == StatementClass::Ddl)
}

pub async fn get(
    Owner(owner): Owner,
    workspace: Workspace,
    ShellCtx(mut shell): ShellCtx,
    Path(path_params): Path<HashMap<String, String>>,
    Query(query_params): Query<RunQuery>,
    Inject(run_store): Inject<dyn QueryRunStore>,
) -> gluon::Result<Response> {
    let run = path_params.get("run").ok_or(gluon::AppError::NotFound)?;
    let run_id = uuid::Uuid::parse_str(run)
        .map(QueryId)
        .map_err(|_| gluon::AppError::NotFound)?;
    let stored = run_store
        .get(&owner, &workspace.profile.id, run_id)
        .ok_or(gluon::AppError::NotFound)?;
    let selected_statement = query_params.statement.unwrap_or(0);
    if stored
        .result
        .as_ref()
        .is_some_and(|result| selected_statement >= result.statements.len())
    {
        return Err(gluon::AppError::BadRequest("Statement index is out of range.".to_owned()));
    }
    let page = query_params.page.unwrap_or(1).max(1);
    shell.title = format!("Query result · {} · pgpaint", workspace.profile_name);
    let run = run_display(&stored, workspace.id(), selected_statement, page, false);
    let mut response = View::new(RunPageProps {
        shell,
        workspace,
        run,
    })
    .into_response();
    if query_params.schema_event == Some(true) && run_schema_changed(&stored.sql) {
        response.headers_mut().insert(
            HeaderName::from_static("hx-trigger"),
            HeaderValue::from_static("pgpaint:schema-changed"),
        );
    }
    Ok(response)
}
