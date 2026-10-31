use std::collections::HashMap;

use axum::{
    body::Body,
    extract::{Form, Path, Query},
    http::HeaderValue,
    response::{IntoResponse, Response},
};
use gluon::prelude::*;
use pgpaint::domain::database::{
    BrowsePage, BrowseRequest, Cell, DatabaseGateway, DbError, DbErrorKind, Filter, FilterOp,
    RelationKind, RelationRef, RowEdit, RowValues, Sort, TableDetail,
};
use pgpaint::web::{encode_url_component, Shell, ShellCtx, Workspace};
use serde::{Deserialize, Serialize};

const PAGE_SIZE: u32 = 50;

#[derive(Deserialize)]
pub struct RowsPath {
    schema: String,
    name: String,
}

#[derive(Serialize)]
struct FilterOption {
    value: &'static str,
    label: &'static str,
    selected: bool,
}

#[derive(Serialize)]
struct GridColumn {
    name: String,
    data_type: String,
    index: usize,
    filter_name: String,
    value_name: String,
    filter_value: String,
    filter_options: Vec<FilterOption>,
    sort_values: String,
    sorted: bool,
    descending: bool,
}

#[derive(Serialize)]
struct EditField {
    index: usize,
    name: String,
    value_name: String,
    null_name: String,
    default_name: String,
    value: String,
    is_null: bool,
    nullable: bool,
    has_default: bool,
    use_default: bool,
}

#[derive(Serialize)]
struct GridCell {
    value: String,
    is_null: bool,
}

#[derive(Serialize)]
struct GridRow {
    id: String,
    cells: Vec<GridCell>,
    key_fields: Vec<EditField>,
    edit_fields: Vec<EditField>,
    editable: bool,
    can_update: bool,
    editing: bool,
    selected: bool,
    edit_values: String,
    cancel_values: String,
}

#[derive(Serialize)]
pub struct RowsProps {
    #[serde(flatten)]
    shell: Shell,
    #[serde(flatten)]
    workspace: Workspace,
    url: String,
    row_only: bool,
    row_id: String,
    row: Option<GridRow>,
    rows: Vec<GridRow>,
    columns: Vec<GridColumn>,
    insert_fields: Vec<EditField>,
    editable: bool,
    page: u64,
    offset: u64,
    row_count: usize,
    sort_column_value: String,
    sort_descending_value: String,
    show_previous: bool,
    previous_values: String,
    next_values: String,
    has_more: bool,
    no_rows: bool,
    estimate_label: String,
    error: Option<DbError>,
}

pub async fn get(
    Path(path): Path<RowsPath>,
    Query(params): Query<HashMap<String, String>>,
    workspace: Workspace,
    ShellCtx(shell): ShellCtx,
    Inject(gateway): Inject<dyn DatabaseGateway>,
) -> gluon::Result<View<RowsProps>> {
    let relation = RelationRef { schema: path.schema, name: path.name };
    let url = rows_url(workspace.id(), &relation);
    let row_only = params.contains_key("edit") || params.contains_key("cancel");
    let editing = params.contains_key("edit");
    let props = load_rows(&*gateway, workspace, shell, relation, url, params, row_only, editing, None).await;
    Ok(View::new(props))
}

pub async fn post(
    Path(path): Path<RowsPath>,
    workspace: Workspace,
    ShellCtx(shell): ShellCtx,
    Inject(gateway): Inject<dyn DatabaseGateway>,
    Form(params): Form<HashMap<String, String>>,
) -> gluon::Result<View<RowsProps>> {
    let relation = RelationRef { schema: path.schema, name: path.name };
    let url = rows_url(workspace.id(), &relation);
    let detail = match gateway.table_detail(&workspace.key, &relation).await {
        Ok(detail) => detail,
        Err(error) => return Ok(View::new(load_rows(&*gateway, workspace, shell, relation, url, params, false, false, Some(error)).await)),
    };
    ensure_editable(&workspace, &detail)?;
    let values = insert_values(&detail, &params);
    let error = gateway.apply_edit(&workspace.key, RowEdit::Insert { relation: relation.clone(), values }).await.err();
    let props = load_rows(&*gateway, workspace, shell, relation, url, params, false, false, error).await;
    Ok(View::new(props))
}

pub async fn put(
    Path(path): Path<RowsPath>,
    workspace: Workspace,
    ShellCtx(shell): ShellCtx,
    Inject(gateway): Inject<dyn DatabaseGateway>,
    Form(params): Form<HashMap<String, String>>,
) -> gluon::Result<Response> {
    let relation = RelationRef { schema: path.schema, name: path.name };
    let url = rows_url(workspace.id(), &relation);
    let detail = match gateway.table_detail(&workspace.key, &relation).await {
        Ok(detail) => detail,
        Err(error) => {
            let props = load_rows(&*gateway, workspace, shell, relation, url, params, true, false, Some(error)).await;
            return Ok(View::new(props).into_response());
        }
    };
    ensure_editable(&workspace, &detail)?;
    let key = key_values(&detail, &params)?;
    let changes = update_values(&detail, &params);
    let result = gateway.apply_edit(&workspace.key, RowEdit::Update { relation: relation.clone(), key, changes }).await;
    let error = match result {
        Ok(1) => None,
        Ok(affected) => Some(DbError::new(DbErrorKind::Query, format!("Expected to update one row, but updated {affected}."))),
        Err(error) => Some(error),
    };
    let props = load_rows(&*gateway, workspace, shell, relation, url, params, true, false, error).await;
    if props.row.is_none() && props.error.is_none() {
        return Ok(delete_response());
    }
    Ok(View::new(props).into_response())
}

pub async fn delete(
    Path(path): Path<RowsPath>,
    workspace: Workspace,
    ShellCtx(shell): ShellCtx,
    Inject(gateway): Inject<dyn DatabaseGateway>,
    Query(params): Query<HashMap<String, String>>,
) -> gluon::Result<Response> {
    let relation = RelationRef { schema: path.schema, name: path.name };
    let url = rows_url(workspace.id(), &relation);
    let detail = match gateway.table_detail(&workspace.key, &relation).await {
        Ok(detail) => detail,
        Err(error) => {
            let props = load_rows(&*gateway, workspace, shell, relation, url, params, false, false, Some(error)).await;
            return Ok(View::new(props).into_response());
        }
    };
    ensure_editable(&workspace, &detail)?;
    let key = key_values(&detail, &params)?;
    match gateway.apply_edit(&workspace.key, RowEdit::Delete { relation: relation.clone(), key }).await {
        Ok(1) => {
            let props = load_rows(&*gateway, workspace, shell, relation, url, params, false, false, None).await;
            Ok(View::new(props).into_response())
        }
        Ok(affected) => {
            let error = DbError::new(DbErrorKind::Query, format!("Expected to delete one row, but deleted {affected}."));
            let props = load_rows(&*gateway, workspace, shell, relation, url, params, false, false, Some(error)).await;
            Ok(View::new(props).into_response())
        }
        Err(error) => {
            let props = load_rows(&*gateway, workspace, shell, relation, url, params, false, false, Some(error)).await;
            Ok(View::new(props).into_response())
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn load_rows(
    gateway: &dyn DatabaseGateway,
    workspace: Workspace,
    shell: Shell,
    relation: RelationRef,
    url: String,
    params: HashMap<String, String>,
    row_only: bool,
    editing: bool,
    forced_error: Option<DbError>,
) -> RowsProps {
    let detail = match gateway.table_detail(&workspace.key, &relation).await {
        Ok(detail) => detail,
        Err(error) => return empty_props(shell, workspace, url, row_only, &params, forced_error.or(Some(error))),
    };
    let editable = can_edit(&workspace, &detail);
    let (filters, sort_column, descending, page) = browse_state(&detail, &params);
    let sort = sort_column.map(|index| Sort { column: detail.columns[index].name.clone(), descending });
    let request = BrowseRequest {
        relation: relation.clone(),
        filters,
        sort: sort.into_iter().collect(),
        offset: page.saturating_mul(u64::from(PAGE_SIZE)),
        limit: PAGE_SIZE,
    };
    let result = gateway.browse(&workspace.key, request).await;
    let (browse, error) = match result {
        Ok(browse) => (Some(browse), forced_error),
        Err(error) => (None, forced_error.or(Some(error))),
    };
    grid_props(shell, workspace, url, &detail, browse.as_ref(), &params, row_only, editing, editable, error, page)
}

fn empty_props(
    shell: Shell,
    workspace: Workspace,
    url: String,
    row_only: bool,
    params: &HashMap<String, String>,
    error: Option<DbError>,
) -> RowsProps {
    RowsProps {
        shell, workspace, url, row_only, row_id: request_row_id(params), row: None, rows: Vec::new(), columns: Vec::new(),
        insert_fields: Vec::new(), editable: false, page: 0, offset: 0, row_count: 0,
        sort_column_value: String::new(), sort_descending_value: "false".to_owned(),
        show_previous: false, previous_values: "{}".to_owned(), next_values: "{}".to_owned(),
        has_more: false, no_rows: true, estimate_label: String::new(), error,
    }
}

#[allow(clippy::too_many_arguments)]
fn grid_props(
    shell: Shell,
    workspace: Workspace,
    url: String,
    detail: &TableDetail,
    browse: Option<&BrowsePage>,
    params: &HashMap<String, String>,
    row_only: bool,
    editing: bool,
    editable: bool,
    error: Option<DbError>,
    page: u64,
) -> RowsProps {
    let (_, sort_column, descending, _) = browse_state(detail, params);
    let columns = detail.columns.iter().enumerate().filter(|(_, column)| !column.hidden).map(|(index, column)| {
        let filter_op = params.get(&format!("f_{index}_op")).map(String::as_str).unwrap_or("");
        let filter_options = filter_choices(filter_op);
        let filter_name = format!("f_{index}_op");
        let value_name = format!("f_{index}_value");
        let filter_value = params.get(&value_name).cloned().unwrap_or_default();
        let sort_values = serde_json::json!({"sort": index, "page": 0}).to_string();
        GridColumn {
            name: column.name.clone(), data_type: column.data_type.clone(), index,
            filter_name, value_name, filter_value, filter_options, sort_values,
            sorted: sort_column == Some(index), descending: descending && sort_column == Some(index),
        }
    }).collect::<Vec<_>>();
    let mut rows = if let Some(browse_page) = browse {
        browse_page.rows.iter().enumerate().map(|(index, cells)| {
            grid_row(detail, &browse_page.columns, cells, params, editable, editing, index)
        }).collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let selected_row = if row_only {
        rows.iter().position(|row| row.selected).map(|index| rows.remove(index))
    } else { None };
    let row_id = selected_row.as_ref().map(|row| row.id.clone()).unwrap_or_else(|| request_row_id(params));
    let insert_fields = insert_fields(detail);
    let show_previous = page > 0;
    let previous_values = serde_json::json!({"page": page.saturating_sub(1)}).to_string();
    let next_values = serde_json::json!({"page": page.saturating_add(1)}).to_string();
    let has_more = browse.is_some_and(|browse| browse.has_more);
    let estimate_label = browse.and_then(|browse| browse.total_estimate).map(|estimate| format!("~{estimate} rows estimated")).unwrap_or_default();
    let no_rows = rows.is_empty();
    let row_count = rows.len();
    RowsProps {
        shell, workspace, url, row_only, row_id, row: selected_row, no_rows, rows, columns,
        insert_fields, editable, page, offset: page.saturating_mul(u64::from(PAGE_SIZE)),
        row_count, sort_column_value: sort_column.map_or_else(String::new, |index| index.to_string()),
        sort_descending_value: descending.to_string(), show_previous, previous_values, next_values,
        has_more, estimate_label, error,
    }
}

fn grid_row(
    detail: &TableDetail,
    browse_columns: &[pgpaint::domain::database::ColumnMeta],
    values: &[Cell],
    params: &HashMap<String, String>,
    editable: bool,
    editing: bool,
    row_index: usize,
) -> GridRow {
    let cells = values.iter().map(|value| GridCell { value: value.clone().unwrap_or_default(), is_null: value.is_none() }).collect::<Vec<_>>();
    let key_fields = detail.primary_key.iter().enumerate().filter_map(|(key_index, name)| {
        browse_columns.iter().position(|column| column.name == *name).and_then(|cell_index| {
            values.get(cell_index).and_then(Clone::clone).map(|value| EditField {
                index: key_index, name: name.clone(), value_name: format!("key_{key_index}"), null_name: String::new(),
                default_name: String::new(), value, is_null: false, nullable: false, has_default: false, use_default: false,
            })
        })
    }).collect::<Vec<_>>();
    let edit_fields = detail.columns.iter().enumerate().filter(|(_, column)| {
        !column.hidden && column.generated.is_none() && !detail.primary_key.contains(&column.name)
    }).filter_map(|(index, column)| browse_columns.iter().position(|meta| meta.name == column.name).and_then(|cell_index| {
        values.get(cell_index).map(|value| EditField {
            index, name: column.name.clone(), value_name: format!("value_{index}"), null_name: format!("null_{index}"),
            default_name: String::new(), value: value.clone().unwrap_or_default(), is_null: value.is_none(),
            nullable: column.nullable, has_default: false, use_default: false,
        })
    })).collect::<Vec<_>>();
    let selected = key_fields.iter().enumerate().all(|(index, key)| params.get(&format!("key_{index}")).is_some_and(|value| value == &key.value))
        && !key_fields.is_empty();
    let mut vals = serde_json::Map::new();
    vals.insert("edit".to_owned(), serde_json::json!("1"));
    for key in &key_fields { vals.insert(key.value_name.clone(), serde_json::json!(key.value)); }
    let edit_values = serde_json::Value::Object(vals).to_string();
    let mut vals = serde_json::Map::new();
    vals.insert("cancel".to_owned(), serde_json::json!("1"));
    for key in &key_fields { vals.insert(key.value_name.clone(), serde_json::json!(key.value)); }
    let cancel_values = serde_json::Value::Object(vals).to_string();
    let complete_key = key_fields.len() == detail.primary_key.len() && !key_fields.is_empty();
    let id = row_id(&key_fields, row_index);
    let can_update = editable && complete_key && !edit_fields.is_empty();
    GridRow { id, cells, key_fields, edit_fields, editable: editable && complete_key, can_update, editing: editing && selected && can_update, selected, edit_values, cancel_values }
}

fn row_id(key_fields: &[EditField], row_index: usize) -> String {
    if key_fields.is_empty() {
        return format!("row-{row_index}");
    }
    let mut id = String::from("row");
    for key in key_fields {
        id.push('-');
        id.push_str(&key.value.len().to_string());
        id.push('-');
        id.push_str(&encode_url_component(&key.value));
    }
    id
}

fn request_row_id(params: &HashMap<String, String>) -> String {
    let mut id = String::from("row");
    let mut found_key = false;
    for index in 0..params.len() {
        if let Some(value) = params.get(&format!("key_{index}")) {
            id.push('-');
            id.push_str(&value.len().to_string());
            id.push('-');
            id.push_str(&encode_url_component(value));
            found_key = true;
        }
    }
    if found_key { id } else { String::new() }
}
fn filter_choices(selected: &str) -> Vec<FilterOption> {
    [
        ("", "No filter"), ("eq", "Equals"), ("not_eq", "Not equal"), ("lt", "Less than"),
        ("lte", "Less than or equal"), ("gt", "Greater than"), ("gte", "Greater than or equal"),
        ("like", "LIKE"), ("ilike", "ILIKE"), ("is_null", "Is NULL"), ("is_not_null", "Is not NULL"),
    ].into_iter().map(|(value, label)| FilterOption { value, label, selected: value == selected }).collect()
}

fn browse_state(detail: &TableDetail, params: &HashMap<String, String>) -> (Vec<Filter>, Option<usize>, bool, u64) {
    let filters = detail.columns.iter().enumerate().filter_map(|(index, column)| {
        let op = parse_filter_op(params.get(&format!("f_{index}_op"))?)?;
        let value = if matches!(op, FilterOp::IsNull | FilterOp::IsNotNull) { None } else { Some(params.get(&format!("f_{index}_value")).cloned().unwrap_or_default()) };
        Some(Filter { column: column.name.clone(), op, value })
    }).collect();
    let current_sort = params.get("sort_column").and_then(|value| value.parse::<usize>().ok())
        .filter(|index| detail.columns.get(*index).is_some_and(|column| !column.hidden));
    let descending = params.get("sort_descending").is_some_and(|value| value == "true");
    let sort_request = params.get("sort").and_then(|value| value.parse::<usize>().ok())
        .filter(|index| detail.columns.get(*index).is_some_and(|column| !column.hidden));
    let (sort_column, descending) = match sort_request {
        Some(index) => (Some(index), current_sort == Some(index) && !descending),
        None => (current_sort, descending),
    };
    let page = params.get("page").and_then(|value| value.parse::<u64>().ok()).unwrap_or(0);
    (filters, sort_column, descending, page)
}

fn parse_filter_op(value: &str) -> Option<FilterOp> {
    match value {
        "eq" => Some(FilterOp::Eq), "not_eq" => Some(FilterOp::NotEq), "lt" => Some(FilterOp::Lt),
        "lte" => Some(FilterOp::Lte), "gt" => Some(FilterOp::Gt), "gte" => Some(FilterOp::Gte),
        "like" => Some(FilterOp::Like), "ilike" => Some(FilterOp::ILike), "is_null" => Some(FilterOp::IsNull),
        "is_not_null" => Some(FilterOp::IsNotNull), _ => None,
    }
}

fn insert_fields(detail: &TableDetail) -> Vec<EditField> {
    detail.columns.iter().enumerate().filter(|(_, column)| !column.hidden && column.generated.is_none()).map(|(index, column)| EditField {
        index, name: column.name.clone(), value_name: format!("value_{index}"), null_name: format!("null_{index}"),
        default_name: format!("default_{index}"), value: String::new(), is_null: false,
        nullable: column.nullable, has_default: column.default.is_some(), use_default: column.default.is_some(),
    }).collect()
}

fn insert_values(detail: &TableDetail, params: &HashMap<String, String>) -> RowValues {
    detail.columns.iter().enumerate()
        .filter(|(_, column)| !column.hidden && column.generated.is_none())
        .filter_map(|(index, column)| {
            if params.contains_key(&format!("default_{index}")) {
                None
            } else if params.contains_key(&format!("value_{index}")) || params.contains_key(&format!("null_{index}")) {
                Some((column.name.clone(), form_cell(params, index)))
            } else {
                None
            }
        }).collect()
}

fn update_values(detail: &TableDetail, params: &HashMap<String, String>) -> RowValues {
    detail.columns.iter().enumerate().filter(|(_, column)| {
        !column.hidden && column.generated.is_none() && !detail.primary_key.contains(&column.name)
    }).filter_map(|(index, column)| {
        (params.contains_key(&format!("value_{index}")) || params.contains_key(&format!("null_{index}")))
            .then(|| (column.name.clone(), form_cell(params, index)))
    }).collect()
}

fn form_cell(params: &HashMap<String, String>, index: usize) -> Cell {
    if params.contains_key(&format!("null_{index}")) { None }
    else { Some(params.get(&format!("value_{index}")).cloned().unwrap_or_default()) }
}

fn key_values(detail: &TableDetail, params: &HashMap<String, String>) -> gluon::Result<RowValues> {
    detail.primary_key.iter().enumerate().map(|(index, column)| {
        let value = params.get(&format!("key_{index}")).cloned().ok_or_else(|| gluon::AppError::BadRequest("The complete primary key is required.".to_owned()))?;
        Ok((column.clone(), Some(value)))
    }).collect()
}

fn can_edit(workspace: &Workspace, detail: &TableDetail) -> bool {
    !workspace.profile.read_only
        && !detail.primary_key.is_empty()
        && matches!(detail.kind, RelationKind::Table | RelationKind::PartitionedTable)
}

fn ensure_editable(workspace: &Workspace, detail: &TableDetail) -> gluon::Result<()> {
    if workspace.profile.read_only {
        return Err(gluon::AppError::Conflict("This connection is read-only.".to_owned()));
    }
    if !matches!(detail.kind, RelationKind::Table | RelationKind::PartitionedTable) {
        return Err(gluon::AppError::BadRequest("Only tables can be edited.".to_owned()));
    }
    if detail.primary_key.is_empty() {
        return Err(gluon::AppError::BadRequest("Row editing requires a primary key.".to_owned()));
    }
    Ok(())
}

fn rows_url(profile_id: &str, relation: &RelationRef) -> String {
    format!("/c/{profile_id}/t/{}/{}/rows", encode_url_component(&relation.schema), encode_url_component(&relation.name))
}


fn delete_response() -> Response {
    let mut response = Response::new(Body::empty());
    response.headers_mut().insert("HX-Reswap", HeaderValue::from_static("delete"));
    response
}
