use axum::{extract::{Path, Query}, response::IntoResponse};
use gluon::prelude::*;
use pgpaint::domain::database::{
    DatabaseGateway, DbError, EngineView, RelationRef, StatementResult, ViewScope,
};
use pgpaint::web::{encode_url_component, Shell, ShellCtx, Workspace};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ViewQuery {
    schema: Option<String>,
    name: Option<String>,
}

#[derive(Serialize)]
pub struct ViewProps {
    #[serde(flatten)]
    shell: Shell,
    #[serde(flatten)]
    workspace: Workspace,
    view_title: String,
    relation_title: Option<String>,
    relations: Vec<RelationOption>,
    relation_picker: bool,
    relations_empty: bool,
    show_refresh: bool,
    result_loaded: bool,
    error: Option<DbError>,
    columns: Vec<GridColumn>,
    rows: Vec<GridRow>,
    truncated: bool,
    is_activity: bool,
    refresh_url: String,
}

#[derive(Serialize)]
struct RelationOption {
    label: String,
    href: String,
}

#[derive(Serialize)]
struct GridColumn {
    label: String,
    type_name: String,
}

#[derive(Serialize)]
struct GridRow {
    cells: Vec<GridCell>,
}

#[derive(Serialize)]
struct GridCell {
    value: String,
    is_null: bool,
}

pub async fn get(
    Path((_id, view_id)): Path<(String, String)>,
    workspace: Workspace,
    ShellCtx(mut shell): ShellCtx,
    Query(query): Query<ViewQuery>,
    Inject(gateway): Inject<dyn DatabaseGateway>,
) -> gluon::Result<axum::response::Response> {
    let view = workspace
        .profile
        .engine
        .views()
        .iter()
        .find(|view| view.id == view_id.as_str())
        .copied()
        .ok_or(gluon::AppError::NotFound)?;
    if !workspace.is_connected() {
        return Err(gluon::AppError::Conflict(
            "Connect to the database to open engine views.".to_owned(),
        ));
    }

    shell.title = format!("{} · {} · pgpaint", view.title, workspace.profile_name);
    let is_activity = matches!(view.id, "activity" | "sessions");
    let refresh_url = format!("/c/{}/views/{}", workspace.id(), view.id);
    let mut props = ViewProps::empty(shell, workspace, view, is_activity, refresh_url);

    match view.scope {
        ViewScope::Database => match gateway.engine_view(&props.workspace.key, view.id, None).await {
            Ok(result) => props.set_result(result),
            Err(error) => props.error = Some(error),
        },
        ViewScope::Relation => {
            let tree = gateway.schema_tree(&props.workspace.key).await?;
            let schema = query.schema.filter(|value| !value.is_empty());
            let name = query.name.filter(|value| !value.is_empty());
            if let (Some(schema), Some(name)) = (schema, name) {
                if !tree.schemas.iter().any(|item| {
                    item.name == schema && item.relations.iter().any(|relation| relation.name == name)
                }) {
                    return Err(gluon::AppError::NotFound);
                }
                let relation = RelationRef { schema: schema.clone(), name: name.clone() };
                props.relation_title = Some(format!("{schema}.{name}"));
                props.relation_picker = false;
                props.show_refresh = true;
                props.refresh_url = format!(
                    "{}?schema={}&name={}",
                    props.refresh_url,
                    encode_url_component(&schema),
                    encode_url_component(&name)
                );
                match gateway
                    .engine_view(&props.workspace.key, view.id, Some(&relation))
                    .await
                {
                    Ok(result) => props.set_result(result),
                    Err(error) => props.error = Some(error),
                }
            } else {
                props.relations = relation_options(&props.workspace, view, &tree.schemas);
                props.relations_empty = props.relations.is_empty();
            }
        }
    }

    Ok(View::new(props).into_response())
}

impl ViewProps {
    fn empty(
        shell: Shell,
        workspace: Workspace,
        view: EngineView,
        is_activity: bool,
        refresh_url: String,
    ) -> Self {
        Self {
            shell,
            workspace,
            view_title: view.title.to_owned(),
            relation_title: None,
            relations: Vec::new(),
            relation_picker: view.scope == ViewScope::Relation,
            relations_empty: false,
            show_refresh: view.scope == ViewScope::Database,
            result_loaded: false,
            error: None,
            columns: Vec::new(),
            rows: Vec::new(),
            truncated: false,
            is_activity,
            refresh_url,
    }
    }

    fn set_result(&mut self, result: StatementResult) {
        self.result_loaded = true;
        self.columns = result
            .columns
            .into_iter()
            .map(|column| GridColumn {
                label: column.name,
                type_name: column.type_name,
            })
            .collect();
        self.rows = result
            .rows
            .into_iter()
            .map(|row| GridRow {
                cells: row
                    .into_iter()
                    .map(|value| GridCell {
                        is_null: value.is_none(),
                        value: value.unwrap_or_default(),
                    })
                    .collect(),
            })
            .collect();
        self.truncated = result.truncated;
    }
}

fn relation_options(
    workspace: &Workspace,
    view: EngineView,
    schemas: &[pgpaint::domain::database::SchemaNode],
) -> Vec<RelationOption> {
    let base = format!("/c/{}/views/{}", workspace.id(), view.id);
    schemas
        .iter()
        .flat_map(|schema| {
            schema.relations.iter().map(|relation| RelationOption {
                label: format!("{}.{}", schema.name, relation.name),
                href: format!(
                    "{base}?schema={}&name={}",
                    encode_url_component(&schema.name),
                    encode_url_component(&relation.name)
                ),
            })
        })
        .collect()
}


