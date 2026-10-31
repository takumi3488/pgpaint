use axum::extract::Query;
use gluon::prelude::*;
use pgpaint::domain::database::{DatabaseGateway, DbError, RelationKind, SchemaTree};
use pgpaint::web::{encode_url_component, Shell, ShellCtx, Workspace};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Default)]
pub struct TreeQuery {
    filter: Option<String>,
}

#[derive(Serialize)]
struct RelationLink {
    name: String,
    href: String,
}

#[derive(Serialize)]
struct RelationGroup {
    label: &'static str,
    relations: Vec<RelationLink>,
}

#[derive(Serialize)]
struct SchemaView {
    name: String,
    groups: Vec<RelationGroup>,
    sequences: Vec<String>,
    sequences_visible: bool,
    routines: Vec<String>,
    routines_visible: bool,
    routines_empty: bool,
}

#[derive(Serialize)]
pub struct TreeProps {
    #[serde(flatten)]
    shell: Shell,
    #[serde(flatten)]
    workspace: Workspace,
    url: String,
    filter: String,
    schemas: Vec<SchemaView>,
    show_empty: bool,
    error: Option<DbError>,
}

pub async fn get(
    workspace: Workspace,
    ShellCtx(mut shell): ShellCtx,
    Inject(gateway): Inject<dyn DatabaseGateway>,
    Query(query): Query<TreeQuery>,
) -> gluon::Result<View<TreeProps>> {
    let filter = query.filter.unwrap_or_default();
    let url = workspace.tree_url.clone();
    shell.title = format!("Schema · {} · pgpaint", workspace.profile_name);
    let schemas = match gateway.schema_tree(&workspace.key).await {
        Ok(tree) => filter_tree(tree, &filter, workspace.id()),
        Err(error) => {
            return Ok(View::new(TreeProps { shell, workspace, url, filter, schemas: Vec::new(), show_empty: false, error: Some(error) }));
        }
    };
    let show_empty = schemas.is_empty();
    Ok(View::new(TreeProps { shell, workspace, url, filter, schemas, show_empty, error: None }))
}

fn filter_tree(tree: SchemaTree, filter: &str, profile_id: &str) -> Vec<SchemaView> {
    let needle = filter.to_lowercase();
    tree.schemas.into_iter().filter_map(|schema| {
        let schema_matches = schema.name.to_lowercase().contains(&needle);
        let groups = [
            ("Tables", RelationKind::Table),
            ("Partitioned tables", RelationKind::PartitionedTable),
            ("Views", RelationKind::View),
            ("Materialized views", RelationKind::MaterializedView),
            ("Foreign tables", RelationKind::ForeignTable),
        ].into_iter().filter_map(|(label, kind)| {
            let relations = schema.relations.iter().filter(|relation| relation.kind == kind)
                .filter(|relation| schema_matches || relation.name.to_lowercase().contains(&needle))
                .map(|relation| RelationLink {
                    name: relation.name.clone(),
                    href: format!("/c/{profile_id}/t/{}/{}", encode_url_component(&schema.name), encode_url_component(&relation.name)),
                }).collect::<Vec<_>>();
            (!relations.is_empty()).then_some(RelationGroup { label, relations })
        }).collect::<Vec<_>>();
        let sequences = schema.sequences.into_iter().filter(|name| schema_matches || name.to_lowercase().contains(&needle)).collect::<Vec<_>>();
        let routines = schema.routines.into_iter().filter(|routine| schema_matches || routine.name.to_lowercase().contains(&needle))
            .map(|routine| format!("{} {}({})", match routine.kind { pgpaint::domain::database::RoutineKind::Function => "Function", pgpaint::domain::database::RoutineKind::Procedure => "Procedure" }, routine.name, routine.arguments))
            .collect::<Vec<_>>();
        let has_content = !groups.is_empty() || !sequences.is_empty() || !routines.is_empty();
        (needle.is_empty() || schema_matches || has_content).then_some(SchemaView {
            name: schema.name, groups, sequences_visible: !sequences.is_empty(), sequences,
            routines_visible: needle.is_empty() || schema_matches || !routines.is_empty(),
            routines_empty: routines.is_empty(), routines,
        })
    }).collect()
}


