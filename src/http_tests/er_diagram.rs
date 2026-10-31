use axum::http::StatusCode;
use pgpaint::domain::{
    connection_profile::{
        ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId, EncryptedSecret,
    },
    database::{Engine, SslMode},
};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use super::{LOCAL_OWNER, TestResult, new_test_app};

#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::too_many_lines)]
async fn graph_caps_tables_filters_columns_and_includes_relationship_kinds() -> TestResult<()> {
    let fixture = new_test_app().await?;
    let schema = format!("er_graph_{}", Uuid::new_v4().simple());
    let inferred_schema = format!("er_inferred_{}", Uuid::new_v4().simple());
    let database_pool = PgPoolOptions::new()
        .connect(&fixture.engine.connection.database_url())
        .await?;
    execute_ddl(&database_pool, format!("CREATE SCHEMA {schema}")).await?;
    execute_ddl(
        &database_pool,
        format!("CREATE TABLE {schema}.parent (id BIGINT PRIMARY KEY)"),
    )
    .await?;
    execute_ddl(
        &database_pool,
        format!(
            "CREATE TABLE {schema}.child_000 (id BIGINT PRIMARY KEY, parent_id BIGINT NOT NULL REFERENCES {schema}.parent(id), optional_text TEXT)"
        ),
    )
    .await?;
    for index in 0..151 {
        execute_ddl(
            &database_pool,
            format!("CREATE TABLE {schema}.table_{index:03} (id BIGINT PRIMARY KEY)"),
        )
        .await?;
    }

    execute_ddl(&database_pool, format!("CREATE SCHEMA {inferred_schema}")).await?;
    execute_ddl(
        &database_pool,
        format!("CREATE TABLE {inferred_schema}.owners (id BIGINT PRIMARY KEY)"),
    )
    .await?;
    execute_ddl(
        &database_pool,
        format!("CREATE TABLE {inferred_schema}.owner_events (owner_id BIGINT NOT NULL)"),
    )
    .await?;
    database_pool.close().await;

    let profile_id = ConnectionProfileId(Uuid::new_v4());
    let profile = ConnectionProfile::new(
        profile_id.clone(),
        Some(LOCAL_OWNER.to_owned()),
        "ER diagram HTTP test".to_owned(),
        Engine::Postgres,
        fixture.engine.connection.host.clone(),
        u32::from(fixture.engine.connection.port),
        fixture.engine.connection.database.clone(),
        fixture.engine.connection.user.clone(),
        ConnectionProfileAuth::Password {
            encrypted_secret: Some(EncryptedSecret::new("test:postgres".to_owned())),
        },
        SslMode::Disable,
        None,
        false,
        None,
        None,
    )?;
    fixture.profiles.save(&profile).await?;
    let base_path = format!("/c/{}/er", profile_id.0);

    let page = fixture
        .client
        .server()
        .get(&format!("{base_path}/{schema}"))
        .await;
    assert_eq!(page.status_code(), StatusCode::OK, "{}", page.text());
    assert!(page.text().contains("data-er-diagram"), "{}", page.text());
    assert!(page.text().contains(&inferred_schema), "{}", page.text());

    let capped = fixture
        .client
        .server()
        .get(&format!("{base_path}/{schema}/graph"))
        .await;
    assert_eq!(capped.status_code(), StatusCode::OK, "{}", capped.text());
    let capped: Value = serde_json::from_str(&capped.text())?;
    assert_eq!(capped["total_tables"], 153);
    assert_eq!(capped["tables"].as_array().map(Vec::len), Some(150));
    let child = capped["tables"]
        .as_array()
        .and_then(|tables| tables.iter().find(|table| table["name"] == "child_000"))
        .ok_or_else(|| std::io::Error::other("capped graph omitted child_000"))?;
    assert_eq!(child["columns"][0]["pk"].as_bool(), Some(true));
    assert_eq!(child["columns"][0]["nullable"].as_bool(), Some(false));
    assert_eq!(child["columns"][1]["fk"].as_bool(), Some(true));
    assert_eq!(child["columns"][1]["nullable"].as_bool(), Some(false));
    assert_eq!(child["columns"][2]["nullable"].as_bool(), Some(true));
    let foreign_key = capped["edges"]
        .as_array()
        .and_then(|edges| {
            edges
                .iter()
                .find(|edge| edge["from_table"] == "child_000" && edge["to_table"] == "parent")
        })
        .ok_or_else(|| std::io::Error::other("capped graph omitted the foreign-key edge"))?;
    assert_eq!(foreign_key["inferred"].as_bool(), Some(false));
    assert_eq!(foreign_key["cardinality"], "N:1");

    let filtered = fixture
        .client
        .server()
        .get(&format!("{base_path}/{schema}/graph?filter=child_000"))
        .await;
    assert_eq!(
        filtered.status_code(),
        StatusCode::OK,
        "{}",
        filtered.text()
    );
    let filtered: Value = serde_json::from_str(&filtered.text())?;
    assert_eq!(filtered["total_tables"], 1);
    assert_eq!(filtered["tables"].as_array().map(Vec::len), Some(1));

    let inferred = fixture
        .client
        .server()
        .get(&format!("{base_path}/{inferred_schema}/graph"))
        .await;
    assert_eq!(
        inferred.status_code(),
        StatusCode::OK,
        "{}",
        inferred.text()
    );
    let inferred: Value = serde_json::from_str(&inferred.text())?;
    let inferred_edge = inferred["edges"]
        .as_array()
        .and_then(|edges| {
            edges
                .iter()
                .find(|edge| edge["from_table"] == "owner_events" && edge["to_table"] == "owners")
        })
        .ok_or_else(|| std::io::Error::other("graph omitted inferred relationship"))?;
    assert_eq!(inferred_edge["inferred"].as_bool(), Some(true));

    Ok(())
}

async fn execute_ddl(pool: &sqlx::PgPool, statement: String) -> Result<(), sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
        .execute(pool)
        .await?;
    Ok(())
}
