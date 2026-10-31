use axum::{Form, response::IntoResponse};
use gluon::prelude::*;
use pgpaint::domain::database::{
    classify, split_statements, DatabaseGateway, DbError, ExplainResult, PlanNode, StatementClass,
};
use pgpaint::web::{Shell, ShellCtx, Workspace};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ExplainForm {
    sql: String,
    analyze: Option<String>,
    confirm: Option<String>,
}

#[derive(Serialize)]
pub struct ExplainProps {
    #[serde(flatten)]
    shell: Shell,
    #[serde(flatten)]
    workspace: Workspace,
    sql: String,
    message: Option<String>,
    error: Option<DbError>,
    confirmation_required: bool,
    has_result: bool,
    plan_available: bool,
    analyzed: bool,
    raw: String,
    plan: Vec<PlanNodeView>,
}

#[derive(Serialize)]
struct PlanNodeView {
    indent: String,
    node_type: String,
    relation: Option<String>,
    estimated_rows: String,
    actual_rows: String,
    misestimated: bool,
    cost: String,
    cost_width: String,
    has_cost: bool,
    actual_time: String,
    time_width: String,
    has_actual_time: bool,
    loops: String,
    details: Vec<PlanDetail>,
    has_details: bool,
}

#[derive(Serialize)]
struct PlanDetail {
    name: String,
    value: String,
}

pub async fn post(
    workspace: Workspace,
    ShellCtx(mut shell): ShellCtx,
    Inject(gateway): Inject<dyn DatabaseGateway>,
    Form(form): Form<ExplainForm>,
) -> gluon::Result<impl IntoResponse> {
    if !workspace.is_connected() {
        return Err(gluon::AppError::Conflict(
            "Connect to the database before explaining a query.".to_owned(),
        ));
    }

    shell.title = format!("Explain · {} · pgpaint", workspace.profile_name);
    let analyze = form.analyze.as_deref().is_some_and(is_truthy);
    let confirm = form.confirm.as_deref().is_some_and(is_truthy);
    let mut props = ExplainProps::empty(shell, workspace, form.sql);

    if split_statements(&props.sql).len() != 1 {
        props.message = Some("Explain accepts exactly one SQL statement.".to_owned());
        return Ok(View::new(props).into_response());
    }

    let class = classify(&props.sql);
    let restriction = analyze_restriction(
        analyze,
        props.workspace.profile.engine.capabilities().explain_analyze,
        class,
        props.workspace.read_only,
        confirm,
    );
    if let Some(restriction) = restriction {
        match restriction {
            AnalyzeRestriction::Unsupported => {
                props.message = Some("This engine does not support EXPLAIN ANALYZE.".to_owned());
            }
            AnalyzeRestriction::ReadOnlyMutation => {
                props.message = Some(
                    "EXPLAIN ANALYZE cannot run DML or DDL on a read-only profile.".to_owned(),
                );
            }
            AnalyzeRestriction::ConfirmationRequired => props.confirmation_required = true,
        }
        return Ok(View::new(props).into_response());
    }

    match gateway.explain(&props.workspace.key, &props.sql, analyze).await {
        Ok(result) => props.set_result(result),
        Err(error) => props.error = Some(error),
    }
    Ok(View::new(props).into_response())
}

impl ExplainProps {
    fn empty(shell: Shell, workspace: Workspace, sql: String) -> Self {
        Self {
            shell,
            workspace,
            sql,
            message: None,
            error: None,
            confirmation_required: false,
            has_result: false,
            plan_available: false,
            analyzed: false,
            raw: String::new(),
            plan: Vec::new(),
        }
    }

    fn set_result(&mut self, result: ExplainResult) {
        self.has_result = true;
        self.analyzed = result.analyzed;
        self.raw = result.raw;
        if let Some(root) = result.plan.as_ref() {
            self.plan = flatten_plan(root);
            self.plan_available = true;
        }
    }
}

fn is_truthy(value: &str) -> bool {
    matches!(value, "1" | "true" | "on" | "yes")
}

#[derive(Debug, PartialEq, Eq)]
enum AnalyzeRestriction {
    Unsupported,
    ReadOnlyMutation,
    ConfirmationRequired,
}

fn analyze_restriction(
    analyze: bool,
    supported: bool,
    class: StatementClass,
    read_only: bool,
    confirmed: bool,
) -> Option<AnalyzeRestriction> {
    if !analyze {
        return None;
    }
    if !supported {
        return Some(AnalyzeRestriction::Unsupported);
    }
    if matches!(class, StatementClass::Dml | StatementClass::Ddl) {
        if read_only {
            return Some(AnalyzeRestriction::ReadOnlyMutation);
        }
        if !confirmed {
            return Some(AnalyzeRestriction::ConfirmationRequired);
        }
    }
    None
}

fn flatten_plan(root: &PlanNode) -> Vec<PlanNodeView> {
    let mut nodes = Vec::new();
    let mut ancestors = Vec::new();
    append_plan(
        root,
        true,
        &mut ancestors,
        root.total_cost,
        root.actual_time_ms,
        &mut nodes,
    );
    nodes
}

fn append_plan(
    node: &PlanNode,
    is_last: bool,
    ancestors: &mut Vec<bool>,
    root_cost: Option<f64>,
    root_time: Option<f64>,
    nodes: &mut Vec<PlanNodeView>,
) {
    let depth = ancestors.len();
    let mut indent = String::new();
    for ancestor_is_last in ancestors.iter() {
        indent.push_str(if *ancestor_is_last { "   " } else { "│  " });
    }
    if depth > 0 {
        indent.push_str(if is_last { "└─ " } else { "├─ " });
    }

    let misestimated = matches!((node.estimated_rows, node.actual_rows), (Some(estimate), Some(actual)) if differs_by_more_than_ten(estimate, actual));
    nodes.push(PlanNodeView {
        indent,
        node_type: node.node_type.clone(),
        relation: node.relation.clone(),
        estimated_rows: format_value(node.estimated_rows),
        actual_rows: format_value(node.actual_rows),
        misestimated,
        cost: format_value(node.total_cost),
        cost_width: bar_width(node.total_cost, root_cost),
        has_cost: node.total_cost.is_some(),
        actual_time: format_value(node.actual_time_ms),
        time_width: bar_width(node.actual_time_ms, root_time),
        has_actual_time: node.actual_time_ms.is_some(),
        loops: node.loops.map_or_else(|| "—".to_owned(), |value| value.to_string()),
        has_details: !node.details.is_empty(),
        details: node
            .details
            .iter()
            .map(|(name, value)| PlanDetail {
                name: name.clone(),
                value: value.clone(),
            })
            .collect(),
    });

    let child_count = node.children.len();
    for (index, child) in node.children.iter().enumerate() {
        ancestors.push(is_last);
        append_plan(
            child,
            index + 1 == child_count,
            ancestors,
            root_cost,
            root_time,
            nodes,
        );
        ancestors.pop();
    }
}

fn format_value(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2}"))
}

fn differs_by_more_than_ten(estimate: f64, actual: f64) -> bool {
    if !estimate.is_finite() || !actual.is_finite() {
        return false;
    }
    match (estimate, actual) {
        (0.0, 0.0) => false,
        (0.0, _) | (_, 0.0) => true,
        _ => estimate / actual > 10.0 || actual / estimate > 10.0,
    }
}

fn bar_width(value: Option<f64>, root: Option<f64>) -> String {
    let width = match (value, root) {
        (Some(value), Some(root)) if value.is_finite() && root.is_finite() && root > 0.0 => {
            (value / root * 100.0).clamp(0.0, 100.0)
        }
        (Some(value), Some(root)) if value.is_finite() && root == 0.0 && value > 0.0 => 100.0,
        _ => 0.0,
    };
    format!("width: {width:.1}%")
}

#[cfg(test)]
mod tests {
    use super::{AnalyzeRestriction, analyze_restriction};
    use pgpaint::domain::database::StatementClass;

    #[test]
    fn read_only_analyze_cannot_be_confirmed() {
        let restriction =
            analyze_restriction(true, true, StatementClass::Dml, true, true);
        assert!(matches!(
            restriction,
            Some(AnalyzeRestriction::ReadOnlyMutation)
        ));
        assert!(matches!(
            analyze_restriction(true, true, StatementClass::Ddl, false, false),
            Some(AnalyzeRestriction::ConfirmationRequired)
        ));
        assert!(
            analyze_restriction(true, true, StatementClass::Ddl, false, true).is_none()
        );
        assert!(matches!(
            analyze_restriction(true, false, StatementClass::Query, false, false),
            Some(AnalyzeRestriction::Unsupported)
        ));
    }

    #[test]
    fn plan_metrics_use_tenfold_threshold_and_root_relative_bars() {
        assert!(!super::differs_by_more_than_ten(10.0, 1.0));
        assert!(super::differs_by_more_than_ten(10.1, 1.0));
        assert_eq!(
            super::bar_width(Some(25.0), Some(100.0)),
            "width: 25.0%"
        );
    }
}

