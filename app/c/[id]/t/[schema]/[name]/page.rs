use axum::extract::{Path, Query};
use gluon::prelude::*;
use pgpaint::domain::database::{DatabaseGateway, DbError, RelationRef, TableDetail, ViewScope};
use pgpaint::web::{encode_url_component, Shell, ShellCtx, Workspace};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct TablePath {
    schema: String,
    name: String,
}

#[derive(Deserialize, Default)]
pub struct TableQuery {
    tab: Option<String>,
}

#[derive(Serialize)]
struct TableTab {
    label: &'static str,
    href: String,
    active: bool,
}

#[derive(Serialize)]
struct RelationViewLink {
    title: &'static str,
    href: String,
}

#[derive(Serialize)]
pub struct TableProps {
    #[serde(flatten)]
    shell: Shell,
    #[serde(flatten)]
    workspace: Workspace,
    detail: Option<TableDetail>,
    table_tabs: Vec<TableTab>,
    rows_url: String,
    relation_views: Vec<RelationViewLink>,
    no_relation_views: bool,
    data_active: bool,
    structure_active: bool,
    ddl_active: bool,
    views_active: bool,
    error: Option<DbError>,
}

pub async fn get(
    Path(path): Path<TablePath>,
    Query(query): Query<TableQuery>,
    workspace: Workspace,
    ShellCtx(mut shell): ShellCtx,
    Inject(gateway): Inject<dyn DatabaseGateway>,
) -> gluon::Result<View<TableProps>> {
    let tab = match query.tab.as_deref() {
        Some("structure") => "structure",
        Some("ddl") => "ddl",
        Some("views") => "views",
        _ => "data",
    };
    shell.title = format!("{}.{} · {} · pgpaint", path.schema, path.name, workspace.profile_name);
    let base = format!("/c/{}/t/{}/{}", workspace.id(), encode_url_component(&path.schema), encode_url_component(&path.name));
    let rows_url = format!("{base}/rows");
    let table_tabs = [
        ("Data", "data"),
        ("Structure", "structure"),
        ("DDL", "ddl"),
        ("Engine views", "views"),
    ]
    .into_iter()
    .map(|(label, key)| TableTab {
        label,
        href: format!("{base}?tab={key}"),
        active: tab == key,
    })
    .collect();
    let relation_views = workspace.profile.engine.views().iter()
        .filter(|view| view.scope == ViewScope::Relation)
        .map(|view| RelationViewLink {
            title: view.title,
            href: format!("/c/{}/views/{}?schema={}&name={}", workspace.id(), encode_url_component(view.id), encode_url_component(&path.schema), encode_url_component(&path.name)),
        }).collect::<Vec<_>>();
    let no_relation_views = relation_views.is_empty();
    let data_active = tab == "data";
    let structure_active = tab == "structure";
    let ddl_active = tab == "ddl";
    let views_active = tab == "views";
    match gateway.table_detail(&workspace.key, &RelationRef { schema: path.schema, name: path.name }).await {
        Ok(detail) => Ok(View::new(TableProps { shell, workspace, detail: Some(detail), table_tabs, rows_url, relation_views, no_relation_views, data_active, structure_active, ddl_active, views_active, error: None })),
        Err(error) => Ok(View::new(TableProps { shell, workspace, detail: None, table_tabs, rows_url, relation_views, no_relation_views, data_active, structure_active, ddl_active, views_active, error: Some(error) })),
    }
}

