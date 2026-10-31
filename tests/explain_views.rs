use std::error::Error;
use std::path::PathBuf;

use aurora_dsql_sqlx_connector::{DsqlConnectOptionsBuilder, Region, SharedCredentialsProvider};
use aws_credential_types::Credentials;
use pgpaint::domain::database::{Engine, PlanNode, StatementResult, ViewScope};
use pgpaint::infrastructure::engines::dialect;
use pgpaint_testkit::{Authentication, ConnectionParams, Database};
use sqlx::postgres::{PgConnectOptions, PgConnection, PgSslMode};
use sqlx::{Connection, Row};
use uuid::Uuid;

const EXPLAIN_SQL: &str = "SELECT c.id, o.id FROM public.customers AS c \
     JOIN public.orders AS o ON o.customer_id = c.id \
     ORDER BY c.full_name, o.id";

#[test]
fn captured_engine_explain_output_parses() {
    let fixtures = [
        (
            Engine::Postgres,
            include_str!("fixtures/explain/postgres/plain.txt"),
            include_str!("fixtures/explain/postgres/analyze.txt"),
        ),
        (
            Engine::CockroachDb,
            include_str!("fixtures/explain/cockroach/plain.txt"),
            include_str!("fixtures/explain/cockroach/analyze.txt"),
        ),
        (
            Engine::YugabyteDb,
            include_str!("fixtures/explain/yugabyte/plain.txt"),
            include_str!("fixtures/explain/yugabyte/analyze.txt"),
        ),
        (
            Engine::AuroraDsql,
            include_str!("fixtures/explain/dsql/plain.txt"),
            include_str!("fixtures/explain/dsql/analyze.txt"),
        ),
    ];

    for (engine, plain, analyze) in fixtures {
        for (fixture, analyzed) in [(plain, false), (analyze, true)] {
            let raw = fixture.strip_suffix('\n').unwrap_or(fixture).to_owned();
            let statement = StatementResult {
                columns: Vec::new(),
                rows: vec![vec![Some(raw.clone())]],
                rows_affected: 0,
                truncated: false,
            };
            let parsed = dialect(engine).parse_plan(&statement, analyzed);
            assert_eq!(parsed.raw, raw, "{engine:?} raw plan changed");
            assert_eq!(parsed.analyzed, analyzed);
            let plan = parsed
                .plan
                .as_ref()
                .unwrap_or_else(|| panic!("{engine:?} fixture did not parse: {raw}"));
            assert!(
                has_node(plan, "join"),
                "{engine:?} fixture has no join: {raw}"
            );
            assert!(
                has_node(plan, "sort"),
                "{engine:?} fixture has no sort: {raw}"
            );
            if raw.to_ascii_lowercase().contains("dpu") {
                assert!(has_detail(plan, "dpu"), "DSQL DPU details were lost: {raw}");
            }
        }
    }
    for engine in Engine::ALL {
        assert!(dialect(engine).view_sql("unknown").is_none());
        let malformed = StatementResult {
            columns: Vec::new(),
            rows: vec![vec![Some("not an execution plan".to_owned())]],
            rows_affected: 0,
            truncated: false,
        };
        assert!(dialect(engine).parse_plan(&malformed, false).plan.is_none());
    }
}

#[test]
fn json_plan_details_keep_server_order() {
    let raw = r#"[{"Plan":{"Node Type":"Sort","Parallel Aware":false,"Startup Cost":1.0,"Plan Rows":1,"Plan Width":8,"Sort Key":["id"],"Plans":[{"Node Type":"Seq Scan","Relation Name":"things","Plan Rows":1,"Total Cost":2.0}]}}]"#;
    let statement = StatementResult {
        columns: Vec::new(),
        rows: vec![vec![Some(raw.to_owned())]],
        rows_affected: 0,
        truncated: false,
    };
    let parsed = dialect(Engine::Postgres).parse_plan(&statement, false);
    assert_eq!(parsed.raw, raw);
    let plan = parsed
        .plan
        .unwrap_or_else(|| panic!("valid JSON plan did not parse: {raw}"));
    assert_eq!(
        plan.details
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["Parallel Aware", "Startup Cost", "Plan Width", "Sort Key"]
    );
    assert_eq!(
        plan.children.first().map(|child| child.node_type.as_str()),
        Some("Seq Scan")
    );
}

#[test]
fn text_plan_parses_metrics_and_preserves_dpu_details() {
    let raw = "DPU estimate: 12\nSort (cost=1.00..3.00 rows=5 width=8) \
        (actual time=0.10..0.20 rows=5 loops=1)\n  -> Hash Join";
    let statement = StatementResult {
        columns: Vec::new(),
        rows: raw
            .lines()
            .map(|line| vec![Some(line.to_owned())])
            .collect(),
        rows_affected: 0,
        truncated: false,
    };
    let parsed = dialect(Engine::AuroraDsql).parse_plan(&statement, true);
    assert_eq!(parsed.raw, raw);
    let plan = parsed
        .plan
        .as_ref()
        .unwrap_or_else(|| panic!("valid text plan did not parse: {raw}"));
    assert_eq!(plan.node_type, "Sort");
    assert_eq!(plan.estimated_rows, Some(5.0));
    assert_eq!(plan.actual_rows, Some(5.0));
    assert_eq!(plan.total_cost, Some(3.0));
    assert_eq!(plan.actual_time_ms, Some(0.2));
    assert_eq!(plan.loops, Some(1));
    assert!(has_detail(plan, "dpu"));
    assert!(has_node(plan, "join"));
}

#[test]
fn dsql_single_row_analyze_output_builds_a_plan_tree() {
    let raw = include_str!("fixtures/explain/dsql/single_row_analyze.txt")
        .trim_end()
        .to_owned();
    let statement = StatementResult {
        columns: Vec::new(),
        rows: vec![vec![Some(raw.clone())]],
        rows_affected: 0,
        truncated: false,
    };
    let parsed = dialect(Engine::AuroraDsql).parse_plan(&statement, true);
    assert_eq!(parsed.raw, raw);
    assert!(parsed.analyzed);
    let plan = parsed
        .plan
        .as_ref()
        .unwrap_or_else(|| panic!("DSQL Result plan did not parse: {raw}"));
    assert_eq!(plan.node_type, "Result");
    assert_eq!(plan.relation, None);
    assert_eq!(plan.estimated_rows, Some(1.0));
    assert_eq!(plan.actual_rows, Some(1.0));
    assert_eq!(plan.total_cost, Some(0.01));
    assert_eq!(plan.actual_time_ms, Some(0.026));
    assert_eq!(plan.loops, Some(1));
    assert!(has_detail(plan, "output"));
}

#[test]
fn cockroach_tree_normalizes_estimates_and_execution_time() {
    let raw = "planning time: 604µs\nexecution time: 51ms\n\n\
        • sort\n\
        │ actual row count: 125,000\n\
        │ estimated row count: 125,000\n\
        └── • lookup join\n\
            │ actual row count: 2\n\
            │ table: public.orders@orders_pkey";
    let statement = StatementResult {
        columns: Vec::new(),
        rows: raw
            .lines()
            .map(|line| vec![Some(line.to_owned())])
            .collect(),
        rows_affected: 0,
        truncated: false,
    };
    let parsed = dialect(Engine::CockroachDb).parse_plan(&statement, true);
    let plan = parsed
        .plan
        .unwrap_or_else(|| panic!("valid Cockroach plan did not parse: {raw}"));
    assert_eq!(plan.node_type, "sort");
    assert_eq!(plan.actual_rows, Some(125_000.0));
    assert_eq!(plan.estimated_rows, Some(125_000.0));
    assert_eq!(plan.actual_time_ms, Some(51.0));
    assert_eq!(
        plan.children
            .first()
            .and_then(|child| child.relation.as_deref()),
        Some("public.orders")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn postgres_views_and_explain_execute() -> Result<(), Box<dyn Error + Send + Sync>> {
    check_engine(Engine::Postgres, pgpaint_testkit::postgres().await?).await
}

#[tokio::test(flavor = "multi_thread")]
async fn cockroach_views_and_explain_execute() -> Result<(), Box<dyn Error + Send + Sync>> {
    check_engine(Engine::CockroachDb, pgpaint_testkit::cockroach().await?).await
}

#[tokio::test(flavor = "multi_thread")]
async fn yugabyte_views_and_explain_execute() -> Result<(), Box<dyn Error + Send + Sync>> {
    check_engine(Engine::YugabyteDb, pgpaint_testkit::yugabyte().await?).await
}

#[tokio::test(flavor = "multi_thread")]
async fn dsql_views_and_explain_execute() -> Result<(), Box<dyn Error + Send + Sync>> {
    check_engine(Engine::AuroraDsql, pgpaint_testkit::dsql().await?).await
}

async fn check_engine(
    engine: Engine,
    database: Database,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    pgpaint_testkit::seed(&database).await?;
    let mut connection = connect(engine, &database.connection).await?;
    let mut errors = check_views(engine, &mut connection).await;
    for analyzed in [false, true] {
        if let Err(error) = check_explain(engine, &mut connection, analyzed).await {
            errors.push(error.to_string());
        }
    }
    if let Err(error) = connection.close().await {
        errors.push(format!("{engine:?} connection failed to close: {error}"));
    }
    if engine == Engine::AuroraDsql
        && let Err(error) = database.shutdown().await
    {
        errors.push(format!("DSQL test container shutdown failed: {error}"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(std::io::Error::other(errors.join("\n")).into())
    }
}

async fn check_views(engine: Engine, connection: &mut PgConnection) -> Vec<String> {
    let mut errors = Vec::new();
    for view in engine.views() {
        let Some(sql) = dialect(engine).view_sql(view.id) else {
            errors.push(format!("{engine:?} has no SQL for view {}", view.id));
            continue;
        };
        let query = match view.scope {
            ViewScope::Database => sqlx::query(sql),
            ViewScope::Relation => sqlx::query(sql).bind("public").bind("customers"),
        };
        match query.fetch_all(&mut *connection).await {
            Ok(rows) if rows.iter().all(|row| !row.is_empty()) => {}
            Ok(_) => errors.push(format!(
                "{engine:?} view {} returned a row without columns",
                view.id
            )),
            Err(error) => errors.push(format!("{engine:?} view {} failed: {error}", view.id)),
        }
    }
    errors
}

async fn check_explain(
    engine: Engine,
    connection: &mut PgConnection,
    analyzed: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let sql = dialect(engine).explain_sql(EXPLAIN_SQL, analyzed);
    let rows = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.as_str()))
        .fetch_all(&mut *connection)
        .await?;
    let mut raw = String::new();
    let mut plan_rows = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let value = row.try_get_raw(0)?.as_str()?.to_owned();
        if index > 0 {
            raw.push('\n');
        }
        raw.push_str(&value);
        plan_rows.push(vec![Some(value)]);
    }
    let statement = StatementResult {
        columns: Vec::new(),
        rows: plan_rows,
        rows_affected: 0,
        truncated: false,
    };
    if std::env::var_os("PGPAINT_CAPTURE_EXPLAIN_FIXTURES").is_some() {
        capture_fixture(engine, analyzed, &raw)?;
    }
    let parsed = dialect(engine).parse_plan(&statement, analyzed);
    if parsed.raw != raw {
        return Err(std::io::Error::other(format!("{engine:?} raw EXPLAIN output changed")).into());
    }
    let plan = parsed.plan.ok_or_else(|| {
        std::io::Error::other(format!(
            "{engine:?} EXPLAIN (analyzed={analyzed}) output did not parse: {raw}"
        ))
    })?;
    let expected_root = plan.node_type.to_ascii_lowercase().contains("sort");
    if !expected_root || !has_node(&plan, "join") || !has_node(&plan, "sort") {
        return Err(std::io::Error::other(format!(
            "{engine:?} unexpected EXPLAIN tree for analyzed={analyzed}: {raw}"
        ))
        .into());
    }
    Ok(())
}

async fn connect(
    engine: Engine,
    params: &ConnectionParams,
) -> Result<PgConnection, Box<dyn Error + Send + Sync>> {
    let mut options = PgConnectOptions::new()
        .host(&params.host)
        .port(params.port)
        .username(&params.user)
        .database(&params.database);
    match &params.auth {
        Authentication::Password { password } => {
            options = options.password(password).ssl_mode(PgSslMode::Disable);
            Ok(PgConnection::connect_with(&options).await?)
        }
        Authentication::DsqlIam {
            region,
            access_key_id,
            secret_access_key,
            ..
        } => {
            if engine != Engine::AuroraDsql {
                return Err(std::io::Error::other(
                    "DSQL IAM credentials supplied for a non-DSQL engine",
                )
                .into());
            }
            let tls = params.tls.as_ref().ok_or_else(|| {
                std::io::Error::other("DSQL test connection has no TLS configuration")
            })?;
            let ca_certificate = tls.ca_certificate_pem.as_deref().ok_or_else(|| {
                std::io::Error::other("DSQL test connection has no CA certificate")
            })?;
            let certificate = TemporaryCertificate::write(ca_certificate)?;
            options = options
                .ssl_mode(PgSslMode::VerifyFull)
                .ssl_root_cert(&certificate.path);
            let credentials = Credentials::new(
                access_key_id,
                secret_access_key,
                None,
                None,
                "pgpaint-explain-test",
            );
            let config = DsqlConnectOptionsBuilder::default()
                .pg_connect_options(options)
                .region(Region::new(region.clone()))
                .credentials_provider(SharedCredentialsProvider::new(credentials))
                .build()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let options = config.authenticated_pg_options().await?;
            Ok(PgConnection::connect_with(&options).await?)
        }
    }
}

struct TemporaryCertificate {
    path: PathBuf,
}

impl TemporaryCertificate {
    fn write(pem: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let path = std::env::temp_dir().join(format!("pgpaint-dsql-ca-{}.pem", Uuid::new_v4()));
        std::fs::write(&path, pem)?;
        Ok(Self { path })
    }
}

impl Drop for TemporaryCertificate {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn capture_fixture(
    engine: Engine,
    analyzed: bool,
    raw: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/explain")
        .join(match engine {
            Engine::Postgres => "postgres",
            Engine::CockroachDb => "cockroach",
            Engine::YugabyteDb => "yugabyte",
            Engine::AuroraDsql => "dsql",
        });
    std::fs::create_dir_all(&directory)?;
    let filename = if analyzed { "analyze.txt" } else { "plain.txt" };
    std::fs::write(directory.join(filename), format!("{raw}\n"))?;
    Ok(())
}

fn has_node(node: &PlanNode, needle: &str) -> bool {
    node.node_type.to_ascii_lowercase().contains(needle)
        || node.children.iter().any(|child| has_node(child, needle))
}
fn has_detail(node: &PlanNode, needle: &str) -> bool {
    node.details.iter().any(|(name, value)| {
        name.to_ascii_lowercase().contains(needle) || value.to_ascii_lowercase().contains(needle)
    }) || node.children.iter().any(|child| has_detail(child, needle))
}
