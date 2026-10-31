use std::{collections::BTreeMap, time::Duration};

use futures::TryStreamExt;
use pgpaint::{
    domain::database::{
        BrowseRequest, ConnectAuth, ConnectTarget, ConnectionKey, DatabaseGateway, DbErrorKind,
        Engine, Filter, FilterOp, OwnerId, QueryId, QueryRequest, RelationRef, RowEdit, RowEvent,
        Secret, Sort, SslMode,
    },
    infrastructure::engines::gateway::SqlxGateway,
};
use pgpaint_testkit::{
    Authentication, ConnectionParams, Database, TlsMode, cockroach, dsql, postgres, seed, yugabyte,
};
use uuid::Uuid;

type GatewayTestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

static GATEWAY_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug)]
struct DsqlGatewayCleanupError {
    contract: Box<dyn std::error::Error + Send + Sync>,
    shutdown: Box<dyn std::error::Error + Send + Sync>,
}

impl std::fmt::Display for DsqlGatewayCleanupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "gateway contract failed: {}; DSQL shutdown failed: {}",
            self.contract, self.shutdown
        )
    }
}

impl std::error::Error for DsqlGatewayCleanupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.contract.as_ref())
    }
}

fn engine_for(database: &Database) -> Option<Engine> {
    match database.kind {
        pgpaint_testkit::DatabaseKind::Postgres => Some(Engine::Postgres),
        pgpaint_testkit::DatabaseKind::Cockroach => Some(Engine::CockroachDb),
        pgpaint_testkit::DatabaseKind::Yugabyte => Some(Engine::YugabyteDb),
        pgpaint_testkit::DatabaseKind::Dsql => Some(Engine::AuroraDsql),
        pgpaint_testkit::DatabaseKind::Metadata => None,
    }
}

fn target(params: &ConnectionParams, engine: Engine, read_only: bool) -> ConnectTarget {
    let auth = match &params.auth {
        Authentication::Password { password } => {
            ConnectAuth::Password(Secret::new(password.clone()))
        }
        Authentication::DsqlIam { region, .. } => ConnectAuth::DsqlIam {
            region: region.clone(),
            admin: params.user == "admin",
        },
    };
    let (ssl_mode, ssl_root_cert_pem) = match &params.tls {
        Some(tls) => (
            match tls.mode {
                TlsMode::Disabled => SslMode::Disable,
                TlsMode::Require => SslMode::Require,
                TlsMode::VerifyFull => SslMode::VerifyFull,
            },
            tls.ca_certificate_pem.clone(),
        ),
        None => (SslMode::Disable, None),
    };
    ConnectTarget {
        engine,
        host: params.host.clone(),
        port: params.port,
        database: params.database.clone(),
        username: params.user.clone(),
        auth,
        ssl_mode,
        ssl_root_cert_pem,
        read_only,
        statement_timeout_ms: None,
    }
}

fn new_key() -> ConnectionKey {
    ConnectionKey {
        owner: OwnerId("gateway-contract-test".to_owned()),
        profile: pgpaint::domain::connection_profile::ConnectionProfileId(Uuid::new_v4()),
    }
}

fn request(sql: impl Into<String>, max_rows: usize, timeout: Option<Duration>) -> QueryRequest {
    QueryRequest {
        id: QueryId(Uuid::new_v4()),
        sql: sql.into(),
        max_rows,
        timeout,
    }
}

fn customer_relation() -> RelationRef {
    RelationRef {
        schema: "public".to_owned(),
        name: "customers".to_owned(),
    }
}

async fn assert_gateway_contract(database: &Database) -> GatewayTestResult<()> {
    let engine = engine_for(database)
        .ok_or_else(|| std::io::Error::other("metadata PostgreSQL is not an engine test target"))?;
    if let Authentication::DsqlIam {
        region,
        access_key_id,
        secret_access_key,
        ..
    } = &database.connection.auth
    {
        // This test and its peer hold `GATEWAY_TEST_LOCK` while touching process-wide AWS settings.
        unsafe {
            std::env::set_var("AWS_ACCESS_KEY_ID", access_key_id);
            std::env::set_var("AWS_SECRET_ACCESS_KEY", secret_access_key);
            std::env::set_var("AWS_REGION", region);
            std::env::remove_var("AWS_SESSION_TOKEN");
        }
    }
    let connect_target = target(&database.connection, engine, false);
    let gateway = SqlxGateway::with_idle_ttl(Duration::from_secs(600));
    let tested = gateway.test(&connect_target).await?;
    assert_eq!(tested.engine, engine);

    let key = new_key();
    let info = gateway.connect(&key, connect_target.clone()).await?;
    assert_eq!(info.engine, engine);
    assert_eq!(
        gateway.server_info(&key).map(|info| info.engine),
        Some(engine)
    );

    assert_query_contract(&gateway, &key, engine).await?;
    assert_cancellation_timeout_contract(&gateway, &key)
        .await
        .map_err(|error| {
            std::io::Error::other(format!("{engine:?} cancellation contract failed: {error}"))
        })?;
    assert_browse_contract(&gateway, &key).await?;
    assert_missing_primary_key_rejected(&gateway, &key).await?;
    let unique = assert_customer_edit_contract(&gateway, &key).await?;
    assert_stream_contract(&gateway, &key).await?;
    let read_only_key = assert_read_only_contract(&gateway, database, engine).await?;

    if engine == Engine::AuroraDsql {
        assert_dsql_multi_statement_contract(&gateway, &key, &unique).await?;
    }

    gateway.disconnect(&read_only_key).await;
    gateway.disconnect(&key).await;
    Ok(())
}

async fn assert_query_contract(
    gateway: &SqlxGateway,
    key: &ConnectionKey,
    engine: Engine,
) -> GatewayTestResult<()> {
    let multi = gateway
        .execute(
            key,
            request("SELECT 1 AS one; SELECT 'two'::text AS two", 10, None),
        )
        .await?;
    assert_eq!(multi.statements.len(), 2);
    assert_eq!(multi.statements[0].rows[0][0].as_deref(), Some("1"));
    assert_eq!(multi.statements[1].rows[0][0].as_deref(), Some("two"));

    let truncated = gateway
        .execute(
            key,
            request(
                "SELECT 1 AS n UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4",
                2,
                None,
            ),
        )
        .await?;
    assert_eq!(truncated.statements[0].rows.len(), 2);
    assert!(truncated.statements[0].truncated);

    let syntax_error = gateway
        .execute(key, request("SELEC 1", 10, None))
        .await
        .err()
        .ok_or_else(|| {
            std::io::Error::other("invalid SQL must be returned as a statement error")
        })?;
    assert_eq!(syntax_error.kind, DbErrorKind::Query);
    assert_eq!(syntax_error.code.as_deref(), Some("42601"));
    if engine == Engine::Postgres {
        assert!(syntax_error.position.is_some());
    }
    Ok(())
}

async fn assert_cancellation_timeout_contract(
    gateway: &SqlxGateway,
    key: &ConnectionKey,
) -> GatewayTestResult<()> {
    let cancel_id = QueryId(Uuid::new_v4());
    let cancel_gateway = gateway.clone();
    let cancel_key = key.clone();
    let query_task = tokio::spawn(async move {
        cancel_gateway
            .execute(
                &cancel_key,
                QueryRequest {
                    id: cancel_id,
                    sql: "SELECT pg_sleep(30)".to_owned(),
                    max_rows: 10,
                    timeout: None,
                },
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(gateway.cancel(key, cancel_id).await?);
    let canceled = tokio::time::timeout(Duration::from_secs(3), query_task)
        .await
        .map_err(|_| std::io::Error::other("canceled query did not finish within three seconds"))??
        .err()
        .ok_or_else(|| std::io::Error::other("canceled query must fail"))?;
    assert_eq!(canceled.kind, DbErrorKind::Canceled);

    let timed_out = gateway
        .execute(
            key,
            request("SELECT pg_sleep(30)", 10, Some(Duration::from_millis(200))),
        )
        .await
        .err()
        .ok_or_else(|| std::io::Error::other("request timeout must interrupt the query"))?;
    assert_eq!(timed_out.kind, DbErrorKind::Timeout);
    Ok(())
}

async fn assert_browse_contract(
    gateway: &SqlxGateway,
    key: &ConnectionKey,
) -> GatewayTestResult<()> {
    let browse = gateway
        .browse(
            key,
            BrowseRequest {
                relation: customer_relation(),
                filters: vec![Filter {
                    column: "active".to_owned(),
                    op: FilterOp::Eq,
                    value: Some("true".to_owned()),
                }],
                sort: vec![Sort {
                    column: "id".to_owned(),
                    descending: false,
                }],
                offset: 0,
                limit: 5,
            },
        )
        .await?;
    assert_eq!(browse.rows.len(), 5);
    assert!(browse.has_more);
    assert_eq!(
        browse.columns.first().map(|column| column.name.as_str()),
        Some("id")
    );
    let first_id = browse.rows[0][0]
        .as_deref()
        .unwrap_or_default()
        .parse::<i64>()?;
    let last_id = browse.rows[4][0]
        .as_deref()
        .unwrap_or_default()
        .parse::<i64>()?;
    assert!(first_id < last_id);
    Ok(())
}

async fn assert_missing_primary_key_rejected(
    gateway: &SqlxGateway,
    key: &ConnectionKey,
) -> GatewayTestResult<()> {
    let missing_pk = gateway
        .apply_edit(
            key,
            RowEdit::Update {
                relation: customer_relation(),
                key: BTreeMap::new(),
                changes: BTreeMap::from([("full_name".to_owned(), Some("Invalid".to_owned()))]),
            },
        )
        .await
        .err()
        .ok_or_else(|| std::io::Error::other("update must require the full primary key"))?;
    assert_eq!(missing_pk.kind, DbErrorKind::Unsupported);
    Ok(())
}

async fn assert_customer_edit_contract(
    gateway: &SqlxGateway,
    key: &ConnectionKey,
) -> GatewayTestResult<String> {
    let unique = Uuid::new_v4().simple().to_string();
    let email = format!("gateway-{unique}@example.test");
    let inserted = gateway
        .apply_edit(
            key,
            RowEdit::Insert {
                relation: customer_relation(),
                values: BTreeMap::from([
                    ("full_name".to_owned(), Some("Gateway Test".to_owned())),
                    ("email".to_owned(), Some(email.clone())),
                ]),
            },
        )
        .await?;
    assert_eq!(inserted, 1);
    let inserted_row = gateway
        .execute(
            key,
            request(
                format!("SELECT id::text, full_name FROM public.customers WHERE email = '{email}'"),
                10,
                None,
            ),
        )
        .await?;
    let id = inserted_row.statements[0].rows[0][0]
        .clone()
        .ok_or_else(|| std::io::Error::other("inserted primary key must be returned"))?;
    assert_eq!(
        inserted_row.statements[0].rows[0][1].as_deref(),
        Some("Gateway Test")
    );
    assert_eq!(
        gateway
            .apply_edit(
                key,
                RowEdit::Update {
                    relation: customer_relation(),
                    key: BTreeMap::from([("id".to_owned(), Some(id.clone()))]),
                    changes: BTreeMap::from([(
                        "full_name".to_owned(),
                        Some("Gateway Updated".to_owned())
                    )]),
                },
            )
            .await?,
        1
    );
    let updated = gateway
        .execute(
            key,
            request(
                format!("SELECT full_name FROM public.customers WHERE id = {id}"),
                10,
                None,
            ),
        )
        .await?;
    assert_eq!(
        updated.statements[0].rows[0][0].as_deref(),
        Some("Gateway Updated")
    );
    assert_eq!(
        gateway
            .apply_edit(
                key,
                RowEdit::Delete {
                    relation: customer_relation(),
                    key: BTreeMap::from([("id".to_owned(), Some(id))]),
                },
            )
            .await?,
        1
    );
    Ok(unique)
}

async fn assert_stream_contract(
    gateway: &SqlxGateway,
    key: &ConnectionKey,
) -> GatewayTestResult<()> {
    let count_result = gateway
        .execute(
            key,
            request("SELECT count(*)::text FROM public.customers", 1, None),
        )
        .await?;
    let expected_count = count_result.statements[0].rows[0][0]
        .as_deref()
        .unwrap_or_default()
        .parse::<usize>()?;
    let mut stream = gateway
        .stream(key, "SELECT id FROM public.customers".to_owned())
        .await?;
    let mut streamed_rows = 0usize;
    let mut got_header = false;
    while let Some(event) = stream.try_next().await? {
        match event {
            RowEvent::Columns(_) => {
                assert!(!got_header, "stream should send one header");
                got_header = true;
            }
            RowEvent::Row(_) => {
                assert!(got_header, "row events must follow the header");
                streamed_rows += 1;
            }
        }
    }
    assert!(got_header);
    assert_eq!(streamed_rows, expected_count);
    Ok(())
}

async fn assert_read_only_contract(
    gateway: &SqlxGateway,
    database: &Database,
    engine: Engine,
) -> GatewayTestResult<ConnectionKey> {
    let read_only_key = new_key();
    let read_only_target = target(&database.connection, engine, true);
    gateway.connect(&read_only_key, read_only_target).await?;
    let write_error = gateway
        .execute(
            &read_only_key,
            request(
                "UPDATE public.customers SET active = false WHERE id = 1",
                1,
                None,
            ),
        )
        .await
        .err()
        .ok_or_else(|| std::io::Error::other("server read-only session must reject writes"))?;
    assert!(matches!(
        write_error.kind,
        DbErrorKind::Query | DbErrorKind::Unsupported
    ));
    let edit_error = gateway
        .apply_edit(
            &read_only_key,
            RowEdit::Delete {
                relation: customer_relation(),
                key: BTreeMap::from([("id".to_owned(), Some("1".to_owned()))]),
            },
        )
        .await
        .err()
        .ok_or_else(|| std::io::Error::other("read-only profile must reject row edits"))?;
    assert_eq!(edit_error.kind, DbErrorKind::Unsupported);
    Ok(read_only_key)
}

async fn assert_dsql_multi_statement_contract(
    gateway: &SqlxGateway,
    key: &ConnectionKey,
    unique: &str,
) -> GatewayTestResult<()> {
    let table = format!("gateway_statement_test_{unique}");
    let statements = gateway
        .execute(
            key,
            request(
                format!(
                    "CREATE TABLE {table} (id INT PRIMARY KEY); INSERT INTO {table} VALUES (1)"
                ),
                10,
                None,
            ),
        )
        .await?;
    assert_eq!(statements.statements.len(), 2);
    gateway
        .execute(key, request(format!("DROP TABLE {table}"), 10, None))
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn gateway_contract_on_postgres_wire_engines() -> GatewayTestResult<()> {
    let _lock = GATEWAY_TEST_LOCK.lock().await;
    let (postgres, cockroach, yugabyte) = tokio::try_join!(postgres(), cockroach(), yugabyte())?;
    tokio::try_join!(seed(&postgres), seed(&cockroach), seed(&yugabyte))?;
    for database in [&postgres, &cockroach, &yugabyte] {
        assert_gateway_contract(database).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn gateway_contract_on_dsql() -> GatewayTestResult<()> {
    let _lock = GATEWAY_TEST_LOCK.lock().await;
    let dsql = dsql().await?;
    let contract = async {
        seed(&dsql).await?;
        assert_gateway_contract(&dsql).await
    }
    .await;
    let shutdown = match dsql.shutdown().await {
        Ok(()) => Ok(()),
        Err(error) => Err(error.into_boxed_dyn_error()),
    };

    match (contract, shutdown) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(contract), Ok(())) => Err(contract),
        (Ok(()), Err(shutdown)) => Err(shutdown),
        (Err(contract), Err(shutdown)) => {
            Err(Box::new(DsqlGatewayCleanupError { contract, shutdown })
                as Box<dyn std::error::Error + Send + Sync>)
        }
    }
}
