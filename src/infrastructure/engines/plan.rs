//! Normalizes PostgreSQL JSON and engine-specific text EXPLAIN output.
use std::fmt;

use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::domain::database::{ExplainResult, PlanNode, StatementResult};

pub(super) fn parse_json(output: &StatementResult, analyzed: bool) -> ExplainResult {
    let raw = raw_plan(output);
    let plan = serde_json::from_str::<OrderedJson>(&raw)
        .ok()
        .and_then(json_plan);
    ExplainResult {
        raw,
        plan,
        analyzed,
    }
}

pub(super) fn parse_text(output: &StatementResult, analyzed: bool) -> ExplainResult {
    let raw = raw_plan(output);
    let plan = parse_text_tree(&raw);
    ExplainResult {
        raw,
        plan,
        analyzed,
    }
}

fn raw_plan(output: &StatementResult) -> String {
    let mut raw = String::new();
    for (index, row) in output.rows.iter().enumerate() {
        if index > 0 {
            raw.push('\n');
        }
        if let Some(Some(value)) = row.first() {
            raw.push_str(value);
        }
    }
    raw
}

fn json_plan(document: OrderedJson) -> Option<PlanNode> {
    let root = match document {
        OrderedJson::Array(plans) => plans.into_iter().next()?,
        other => other,
    };
    let wrapped = root.get("Plan").is_some();
    let mut plan = json_node(root.get("Plan").unwrap_or(&root))?;
    if wrapped {
        for (key, value) in root.as_object()? {
            if key != "Plan" {
                plan.details.push((key.clone(), value.display()));
            }
        }
    }
    Some(plan)
}

fn json_node(value: &OrderedJson) -> Option<PlanNode> {
    let fields = value.as_object()?;
    let node_type = value.string("Node Type")?.to_owned();
    let relation = value
        .string("Relation Name")
        .or_else(|| value.string("Table Name"))
        .map(str::to_owned);
    let estimated_rows = value
        .number("Plan Rows")
        .or_else(|| value.number("Estimated Rows"));
    let actual_rows = value.number("Actual Rows");
    let total_cost = value.number("Total Cost");
    let actual_time_ms = value
        .number("Actual Total Time")
        .or_else(|| value.number("Actual Time"));
    let loops = value.number("Actual Loops").and_then(float_u64);
    let children = match value.get("Plans") {
        Some(plans) => plans
            .as_array()?
            .iter()
            .map(json_node)
            .collect::<Option<Vec<_>>>()?,
        None => Vec::new(),
    };
    let details = fields
        .iter()
        .filter(|(key, _)| {
            !matches!(
                key.as_str(),
                "Node Type"
                    | "Relation Name"
                    | "Table Name"
                    | "Plan Rows"
                    | "Estimated Rows"
                    | "Actual Rows"
                    | "Total Cost"
                    | "Actual Total Time"
                    | "Actual Time"
                    | "Actual Loops"
                    | "Plans"
            )
        })
        .map(|(key, value)| (key.clone(), value.display()))
        .collect();
    Some(PlanNode {
        node_type,
        relation,
        estimated_rows,
        actual_rows,
        total_cost,
        actual_time_ms,
        loops,
        details,
        children,
    })
}

fn parse_text_tree(raw: &str) -> Option<PlanNode> {
    let mut roots = Vec::new();
    let mut stack: Vec<(usize, PlanNode)> = Vec::new();
    let mut prelude = Vec::new();

    for line in raw.lines() {
        let text = strip_tree_prefix(line).trim();
        if text.is_empty() || is_text_header(text) {
            continue;
        }
        if let Some((depth, node)) = text_node(line) {
            while stack
                .last()
                .is_some_and(|(parent_depth, _)| *parent_depth >= depth)
            {
                append_top(&mut stack, &mut roots);
            }
            stack.push((depth, node));
        } else if is_dpu_detail(text) {
            let detail = ("DPU estimate".to_owned(), text.to_owned());
            if let Some((_, root)) = stack.first_mut() {
                root.details.push(detail);
            } else {
                prelude.push(detail);
            }
        } else if is_root_summary(text) {
            if let Some((_, root)) = stack.first_mut() {
                add_text_detail(root, text);
            } else if let Some((key, value)) = text.split_once(':') {
                prelude.push((key.trim().to_owned(), value.trim().to_owned()));
            }
        } else if let Some((_, parent)) = stack.last_mut() {
            add_text_detail(parent, text);
        } else {
            let (key, value) = text
                .split_once(':')
                .map_or(("detail", text), |(key, value)| (key.trim(), value.trim()));
            prelude.push((key.to_owned(), value.to_owned()));
        }
    }
    while !stack.is_empty() {
        append_top(&mut stack, &mut roots);
    }
    if roots.len() != 1 {
        return None;
    }
    let mut root = roots.remove(0);
    if !prelude.is_empty() {
        for (key, value) in &prelude {
            if root.actual_time_ms.is_none() && key.eq_ignore_ascii_case("execution time") {
                root.actual_time_ms = parse_duration_ms(value);
            }
        }
        prelude.append(&mut root.details);
        root.details = prelude;
    }
    Some(root)
}

fn add_text_detail(parent: &mut PlanNode, text: &str) {
    let (key, value) = text
        .split_once(':')
        .map_or(("detail", text), |(key, value)| (key.trim(), value.trim()));
    let key_lower = key.to_ascii_lowercase();
    if key_lower == "table" {
        parent.relation = value
            .split('@')
            .next()
            .map(str::trim)
            .filter(|relation| !relation.is_empty())
            .map(str::to_owned);
    } else if key_lower.starts_with("estimated row") || key_lower.starts_with("estimated rows") {
        parent.estimated_rows = parse_first_number(value);
    } else if key_lower.starts_with("actual row") || key_lower.starts_with("actual rows") {
        parent.actual_rows = parse_first_number(value);
    } else if key_lower.starts_with("total cost") {
        parent.total_cost = parse_first_number(value);
    } else if key_lower.starts_with("actual total time")
        || key_lower.starts_with("actual time")
        || key_lower == "execution time"
    {
        parent.actual_time_ms = parse_duration_ms(value);
    } else if key_lower == "loops" {
        parent.loops = parse_first_number(value).and_then(float_u64);
    }
    parent.details.push((key.to_owned(), value.to_owned()));
}

fn parse_duration_ms(value: &str) -> Option<f64> {
    for (unit, multiplier) in [
        ("ms", 1.0),
        ("µs", 0.001),
        ("us", 0.001),
        ("ns", 0.000_001),
        ("s", 1_000.0),
    ] {
        if let Some(number) = value.trim().strip_suffix(unit) {
            return number
                .trim()
                .parse::<f64>()
                .ok()
                .map(|number| number * multiplier);
        }
    }
    parse_first_number(value)
}

fn append_top(stack: &mut Vec<(usize, PlanNode)>, roots: &mut Vec<PlanNode>) {
    let Some((_, child)) = stack.pop() else {
        return;
    };
    if let Some((_, parent)) = stack.last_mut() {
        parent.children.push(child);
    } else {
        roots.push(child);
    }
}

fn text_node(line: &str) -> Option<(usize, PlanNode)> {
    let marker =
        line.contains('•') || line.contains("├──") || line.contains("└──") || line.contains("->");
    let indentation = line.chars().take_while(|ch| *ch == ' ').count();
    let depth = if marker && !line.contains("->") {
        indentation / 4
            + line
                .chars()
                .filter(|ch| matches!(ch, '│' | '├' | '└'))
                .count()
    } else {
        indentation / 2
    };
    let text = strip_tree_prefix(line).trim();
    if text.is_empty() || is_text_header(text) {
        return None;
    }
    let leading_type = text
        .split_once(" (cost=")
        .or_else(|| text.split_once(" (actual time="))
        .map_or(text, |(node, _)| node)
        .trim();
    let operator = leading_type
        .split_once(" on ")
        .map_or(leading_type, |(node, _)| node);
    let (operator, annotation) = operator
        .split_once(" (")
        .map_or((operator, None), |(node, details)| {
            (node, Some(details.trim_end_matches(')').trim()))
        });
    let (operator, access_method) = operator
        .split_once(" using ")
        .map_or((operator, None), |(node, method)| (node, Some(method)));
    let node_type = operator.trim();
    if !marker && !has_plan_operator(node_type) {
        return None;
    }
    if node_type.is_empty() || node_type.len() > 160 {
        return None;
    }

    let relation = leading_type
        .split_once(" on ")
        .and_then(|(_, relation)| relation.split_whitespace().next())
        .map(|relation| {
            relation
                .trim_matches(|ch: char| ch == ',' || ch == ')')
                .to_owned()
        });
    let estimated_rows = metric(text, "rows=");
    let actual_rows = metric_after(text, "actual time=", "rows=");
    let total_cost = text
        .split_once("cost=")
        .and_then(|(_, rest)| rest.split_once(".."))
        .and_then(|(_, rest)| parse_first_number(rest));
    let actual_time_ms = text
        .split_once("actual time=")
        .and_then(|(_, rest)| rest.split_once(".."))
        .and_then(|(_, rest)| parse_first_number(rest));
    let loops = metric_after(text, "actual time=", "loops=").and_then(float_u64);
    let mut details = Vec::new();
    if let Some(method) = access_method {
        details.push(("using".to_owned(), method.trim().to_owned()));
    }
    if let Some(annotation) = annotation {
        details.push(("plan".to_owned(), annotation.to_owned()));
    }
    Some((
        depth,
        PlanNode {
            node_type: node_type.to_owned(),
            relation,
            estimated_rows,
            actual_rows,
            total_cost,
            actual_time_ms,
            loops,
            details,
            children: Vec::new(),
        },
    ))
}

fn strip_tree_prefix(line: &str) -> &str {
    let mut text = line.trim_start();
    loop {
        if let Some(rest) = text.strip_prefix("•") {
            text = rest.trim_start();
        } else if let Some(rest) = text.strip_prefix("├──") {
            text = rest.trim_start();
        } else if let Some(rest) = text.strip_prefix("└──") {
            text = rest.trim_start();
        } else if let Some(rest) = text.strip_prefix("│") {
            text = rest.trim_start();
        } else if let Some(rest) = text.strip_prefix("->") {
            text = rest.trim_start();
        } else {
            break;
        }
    }
    text
}

fn has_plan_operator(text: &str) -> bool {
    const OPERATORS: &[&str] = &[
        "root",
        "scan",
        "sort",
        "aggregate",
        "render",
        "lookup join",
        "lookup",
        "limit",
        "distinct",
        "filter",
        "union",
        "window",
        "group",
        "hash join",
        "merge join",
        "nested loop",
        "index only scan",
        "index scan",
        "seq scan",
        "bitmap heap scan",
        "bitmap index scan",
        "parallel seq scan",
        "index join",
        "apply join",
        "zigzag join",
        "inverted join",
        "join",
        "gather",
        "append",
        "unique",
        "result",
        "top k",
        "values",
        "project set",
        "project",
        "table reader",
        "index reader",
        "materialize",
        "memoize",
        "subquery scan",
        "cte scan",
        "function scan",
        "distributed",
        "ordinality",
    ];
    let lower = text.to_ascii_lowercase();
    lower == "hash"
        || lower.starts_with("hash (")
        || OPERATORS.iter().any(|operator| {
            lower.strip_prefix(operator).is_some_and(|suffix| {
                suffix.is_empty() || suffix.starts_with(' ') || suffix.starts_with('(')
            })
        })
}
fn is_text_header(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "info" | "query plan" | "plan"
    )
}

fn is_root_summary(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.starts_with("planning time:") || lower.starts_with("execution time:")
}

fn is_dpu_detail(text: &str) -> bool {
    text.to_ascii_lowercase().contains("dpu")
}

fn metric(text: &str, key: &str) -> Option<f64> {
    text.split_once(key)
        .and_then(|(_, rest)| parse_first_number(rest))
}

fn metric_after(text: &str, anchor: &str, key: &str) -> Option<f64> {
    text.split_once(anchor)
        .and_then(|(_, rest)| metric(rest, key))
}

fn parse_first_number(text: &str) -> Option<f64> {
    let token = text
        .trim_start()
        .split(|ch: char| ch.is_whitespace() || ch == ')')
        .next()?;
    token.replace(',', "").parse().ok()
}
fn float_u64(value: f64) -> Option<u64> {
    if value.is_finite() && value >= 0.0 && value.fract() == 0.0 {
        value.to_string().parse().ok()
    } else {
        None
    }
}

#[derive(Debug)]
enum OrderedJson {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<OrderedJson>),
    Object(Vec<(String, OrderedJson)>),
}

impl OrderedJson {
    fn as_object(&self) -> Option<&[(String, Self)]> {
        match self {
            Self::Object(fields) => Some(fields),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    fn get(&self, key: &str) -> Option<&Self> {
        self.as_object()?
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    fn string(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    fn number(&self, key: &str) -> Option<f64> {
        match self.get(key)? {
            Self::Number(value) => value.parse().ok(),
            _ => None,
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Null => "null".to_owned(),
            Self::Bool(value) => value.to_string(),
            Self::Number(value) | Self::String(value) => value.clone(),
            Self::Array(items) => format!(
                "[{}]",
                items.iter().map(Self::json).collect::<Vec<_>>().join(",")
            ),
            Self::Object(fields) => format!(
                "{{{}}}",
                fields
                    .iter()
                    .map(|(key, value)| format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_default(),
                        value.json()
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }

    fn json(&self) -> String {
        match self {
            Self::String(value) => serde_json::to_string(value).unwrap_or_default(),
            other => other.display(),
        }
    }
}

impl<'de> Deserialize<'de> for OrderedJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct JsonVisitor;

        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = OrderedJson;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON value")
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(OrderedJson::Null)
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(OrderedJson::Null)
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(OrderedJson::Bool(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(OrderedJson::Number(value.to_string()))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(OrderedJson::Number(value.to_string()))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
                Ok(OrderedJson::Number(value.to_string()))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(OrderedJson::String(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(OrderedJson::String(value))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element()? {
                    values.push(value);
                }
                Ok(OrderedJson::Array(values))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut fields = Vec::new();
                while let Some(field) = map.next_entry()? {
                    fields.push(field);
                }
                Ok(OrderedJson::Object(fields))
            }
        }

        deserializer.deserialize_any(JsonVisitor)
    }
}
