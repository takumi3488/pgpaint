use axum::{extract::Path, response::{IntoResponse, Response}};
use gluon::prelude::*;
use pgpaint::domain::database::DatabaseGateway;
use pgpaint::web::{encode_url_component, Shell, ShellCtx, Workspace};
use serde::Serialize;

#[derive(Serialize)]
pub struct ErDiagramProps {
    #[serde(flatten)]
    shell: Shell,
    #[serde(flatten)]
    workspace: Workspace,
    schema: String,
    schemas: Vec<SchemaOption>,
    graph_url: String,
    table_url_prefix: String,
}

#[derive(Serialize)]
pub struct SchemaOption {
    name: String,
    href: String,
    selected: bool,
}

pub async fn get(
    Path((id, schema)): Path<(String, String)>,
    workspace: Workspace,
    ShellCtx(mut shell): ShellCtx,
    Inject(gateway): Inject<dyn DatabaseGateway>,
) -> gluon::Result<Response> {
    if !workspace.is_connected() {
        return Err(gluon::AppError::Conflict("Connect to the database to view its ER diagram.".to_owned()));
    }

    let tree = gateway.schema_tree(&workspace.key).await?;
    let schemas = tree
        .schemas
        .iter()
        .map(|item| SchemaOption {
            name: item.name.clone(),
            href: format!("/c/{id}/er/{}", encode_url_component(&item.name)),
            selected: item.name == schema,
        })
        .collect::<Vec<_>>();
    if !schemas.iter().any(|item| item.selected) {
        return Err(gluon::AppError::NotFound);
    }

    shell.title = format!("ER diagram · {schema} · pgpaint");
    Ok(View::new(ErDiagramProps {
        shell,
        workspace,
        graph_url: format!("/c/{id}/er/{}/graph", encode_url_component(&schema)),
        table_url_prefix: format!("/c/{id}/t/"),
        schema,
        schemas,
    })
    .into_response())
}
