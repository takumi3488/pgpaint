use std::collections::HashMap;

use axum::{
    body::{Body, Bytes},
    extract::{Path, Query},
    http::{HeaderValue, header},
    response::Response,
};
use futures::StreamExt;
use gluon::prelude::*;
use pgpaint::domain::database::{DatabaseGateway, DbError, DbErrorKind, RowEvent, StatementClass, classify, split_statements};
use pgpaint::domain::{database::QueryId, query_run::{QueryRunStatus, QueryRunStore}};
use pgpaint::web::{Owner, Workspace};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct ExportQuery {
    statement: Option<usize>,
    format: Option<String>,
}

pub async fn get(
    Owner(owner): Owner,
    workspace: Workspace,
    Path(path_params): Path<HashMap<String, String>>,
    Query(params): Query<ExportQuery>,
    Inject(database): Inject<dyn DatabaseGateway>,
    Inject(run_store): Inject<dyn QueryRunStore>,
) -> gluon::Result<Response> {
    let run_id = path_params
        .get("run")
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .map(QueryId)
        .ok_or(gluon::AppError::NotFound)?;
    let run = run_store
        .get(&owner, &workspace.profile.id, run_id)
        .ok_or(gluon::AppError::NotFound)?;
    if run.status != QueryRunStatus::Succeeded {
        return Err(gluon::AppError::Conflict("Only completed query runs can be exported.".to_owned()));
    }
    let spans = split_statements(&run.sql);
    let statement_index = params.statement.unwrap_or(0);
    let statement = spans
        .get(statement_index)
        .ok_or_else(|| gluon::AppError::BadRequest("Statement index is out of range.".to_owned()))?;
    if classify(statement.text) != StatementClass::Query {
        return Err(gluon::AppError::BadRequest("Only read-only query statements can be exported.".to_owned()));
    }
    let format = params.format.as_deref().unwrap_or("csv");
    if !matches!(format, "csv" | "json") {
        return Err(gluon::AppError::BadRequest("Export format must be csv or json.".to_owned()));
    }
    let mut rows = database
        .stream(&workspace.key, statement.text.trim().to_owned())
        .await?;
    let json_format = format == "json";
    let output: futures::stream::BoxStream<'static, std::result::Result<Bytes, DbError>> =
        Box::pin(async_stream::try_stream! {
        let mut first_row = true;
        let mut wrote_columns = false;
        while let Some(event) = rows.next().await {
            match event? {
                RowEvent::Columns(columns) => {
                    if json_format {
                        let columns = columns.iter().map(|column| json!({
                            "name": column.name,
                            "type": column.type_name,
                        })).collect::<Vec<_>>();
                        let encoded = serde_json::to_vec(&columns)
                            .map_err(|error| DbError::new(DbErrorKind::Internal, error.to_string()))?;
                        yield Bytes::from_static(b"{\"columns\":");
                        yield Bytes::from(encoded);
                        yield Bytes::from_static(b",\"rows\":[");
                    } else {
                        let mut record = Vec::new();
                        for (index, column) in columns.iter().enumerate() {
                            if index > 0 { record.push(b','); }
                            append_csv_field(&mut record, &column.name);
                        }
                        record.push(b'\n');
                        yield Bytes::from(record);
                    }
                    wrote_columns = true;
                }
                RowEvent::Row(row) => {
                    if json_format {
                        if !first_row { yield Bytes::from_static(b","); }
                        let encoded = serde_json::to_vec(&row)
                            .map_err(|error| DbError::new(DbErrorKind::Internal, error.to_string()))?;
                        yield Bytes::from(encoded);
                        first_row = false;
                    } else {
                        let mut record = Vec::new();
                        for (index, cell) in row.iter().enumerate() {
                            if index > 0 { record.push(b','); }
                            if let Some(value) = cell { append_csv_field(&mut record, value); }
                        }
                        record.push(b'\n');
                        yield Bytes::from(record);
                    }
                }
            }
        }
        if json_format {
            if !wrote_columns { yield Bytes::from_static(b"{\"columns\":[],\"rows\":["); }
            yield Bytes::from_static(b"]}");
        }
        });
    let mut response = Response::new(Body::from_stream(output));
    let content_type = if format == "csv" { "text/csv; charset=utf-8" } else { "application/json" };
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"query-{}-statement-{statement_index}.{format}\"", run_id.0))
            .map_err(|error| gluon::AppError::Internal(Box::new(error)))?,
    );
    Ok(response)
}

fn append_csv_field(output: &mut Vec<u8>, value: &str) {
    output.push(b'"');
    for byte in value.bytes() {
        output.push(byte);
        if byte == b'"' { output.push(b'"'); }
    }
    output.push(b'"');
}
