use axum::http::StatusCode;
use pgpaint::domain::{
    connection_profile::{
        ConnectionProfile, ConnectionProfileAuth, ConnectionProfileId, EncryptedSecret,
    },
    database::{Engine, SslMode},
};
use pgpaint_testkit::{Authentication, seed};
use uuid::Uuid;

use super::{LOCAL_OWNER, TestApp, TestResult, csrf_token, new_test_app};

#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread")]
async fn schema_tree_table_detail_rows_and_edits_work() -> TestResult<()> {
    let fixture = new_test_app().await?;
    seed(&fixture.engine).await?;
    let profile_id = save_profile(&fixture, false).await?;
    let read_only_profile_id = save_profile(&fixture, true).await?;

    let home = fixture.client.server().get("/").await;
    assert_eq!(home.status_code(), StatusCode::OK, "{}", home.text());
    let csrf = csrf_token(&home.text())?;
    let cookie = home
        .cookies()
        .get("gluon_sid")
        .map(|cookie| format!("gluon_sid={}", cookie.value()))
        .ok_or_else(|| std::io::Error::other("session cookie missing"))?;

    let tree_path = format!("/c/{}/tree", profile_id.0);
    let tree = fixture
        .client
        .server()
        .get(&tree_path)
        .add_header("cookie", cookie.clone())
        .add_header("hx-request", "true")
        .await;
    assert_eq!(tree.status_code(), StatusCode::OK, "{}", tree.text());
    assert!(tree.text().contains("customers"), "{}", tree.text());
    assert!(tree.text().contains("Sequences"), "{}", tree.text());
    assert!(tree.text().contains("Routines"), "{}", tree.text());
    assert!(
        tree.text().contains("pgpaint:schema-changed"),
        "{}",
        tree.text()
    );

    let filtered_tree = fixture
        .client
        .server()
        .get(&format!("{tree_path}?filter=customers"))
        .add_header("cookie", cookie.clone())
        .add_header("hx-request", "true")
        .await;
    assert_eq!(
        filtered_tree.status_code(),
        StatusCode::OK,
        "{}",
        filtered_tree.text()
    );
    assert!(
        filtered_tree.text().contains("customers"),
        "{}",
        filtered_tree.text()
    );
    assert!(
        !filtered_tree.text().contains("order_items"),
        "{}",
        filtered_tree.text()
    );

    let structure = fixture
        .client
        .server()
        .get(&format!(
            "/c/{}/t/public/customers?tab=structure",
            profile_id.0
        ))
        .add_header("cookie", cookie.clone())
        .add_header("hx-request", "true")
        .await;
    assert_eq!(
        structure.status_code(),
        StatusCode::OK,
        "{}",
        structure.text()
    );
    assert!(structure.text().contains("Columns"), "{}", structure.text());
    assert!(
        structure.text().contains("Primary key"),
        "{}",
        structure.text()
    );
    assert!(structure.text().contains("Indexes"), "{}", structure.text());
    assert!(
        structure.text().contains("Foreign keys"),
        "{}",
        structure.text()
    );

    let ddl = fixture
        .client
        .server()
        .get(&format!("/c/{}/t/public/customers?tab=ddl", profile_id.0))
        .add_header("cookie", cookie.clone())
        .add_header("hx-request", "true")
        .await;
    assert_eq!(ddl.status_code(), StatusCode::OK, "{}", ddl.text());
    assert!(
        ddl.text().contains("data-readonly=\"true\""),
        "{}",
        ddl.text()
    );
    assert!(ddl.text().contains("CREATE TABLE"), "{}", ddl.text());

    let rows_path = format!("/c/{}/t/public/customers/rows", profile_id.0);
    let rows = fixture
        .client
        .server()
        .get(&rows_path)
        .add_header("cookie", cookie.clone())
        .add_header("hx-request", "true")
        .await;
    assert_eq!(rows.status_code(), StatusCode::OK, "{}", rows.text());
    assert!(rows.text().contains("Filter full_name"), "{}", rows.text());
    assert!(rows.text().contains("Insert row"), "{}", rows.text());
    assert!(rows.text().contains("Next"), "{}", rows.text());

    let filtered_rows = fixture
        .client
        .server()
        .get(&format!("{rows_path}?f_1_op=eq&f_1_value=Customer%201"))
        .add_header("cookie", cookie.clone())
        .add_header("hx-request", "true")
        .await;
    assert_eq!(
        filtered_rows.status_code(),
        StatusCode::OK,
        "{}",
        filtered_rows.text()
    );
    assert!(
        filtered_rows.text().contains("Customer 1"),
        "{}",
        filtered_rows.text()
    );
    assert!(
        !filtered_rows.text().contains("Customer 10"),
        "{}",
        filtered_rows.text()
    );
    let edit_row = fixture
        .client
        .server()
        .get(&format!(
            "{rows_path}?f_1_op=eq&f_1_value=Customer%201&edit=1&key_0=1"
        ))
        .add_header("cookie", cookie.clone())
        .add_header("hx-request", "true")
        .await;
    assert_eq!(
        edit_row.status_code(),
        StatusCode::OK,
        "{}",
        edit_row.text()
    );
    assert!(edit_row.text().contains("hx-put="), "{}", edit_row.text());
    assert!(edit_row.text().contains("Save"), "{}", edit_row.text());

    let row_name = "Schema HTTP insert";
    let row_email = "schema-http-insert@example.test";
    let inserted = fixture
        .client
        .server()
        .post(&rows_path)
        .add_header("cookie", cookie.clone())
        .add_header("x-csrf-token", csrf.clone())
        .add_header("hx-request", "true")
        .form(&[
            ("value_1", row_name),
            ("value_2", row_email),
            ("default_3", "true"),
            ("default_4", "true"),
            ("default_5", "true"),
            ("f_1_op", "eq"),
            ("f_1_value", row_name),
            ("page", "0"),
            ("sort_column", ""),
            ("sort_descending", "false"),
        ])
        .await;
    assert_eq!(
        inserted.status_code(),
        StatusCode::OK,
        "{}",
        inserted.text()
    );
    assert!(inserted.text().contains(row_name), "{}", inserted.text());

    let engine_pool = sqlx::PgPool::connect(&fixture.engine.connection.database_url()).await?;
    let customer_id: i64 = sqlx::query_scalar("SELECT id FROM public.customers WHERE email = $1")
        .bind(row_email)
        .fetch_one(&engine_pool)
        .await?;
    let customer_id = customer_id.to_string();
    let updated_email = "schema-http-updated@example.test";
    let updated = fixture
        .client
        .server()
        .put(&rows_path)
        .add_header("cookie", cookie.clone())
        .add_header("x-csrf-token", csrf.clone())
        .add_header("hx-request", "true")
        .form(&[
            ("key_0", customer_id.as_str()),
            ("value_1", row_name),
            ("value_2", updated_email),
            ("value_3", "2025-01-01 00:00:00+00"),
            ("value_4", "{}"),
            ("value_5", "true"),
            ("f_1_op", "eq"),
            ("f_1_value", row_name),
            ("page", "0"),
            ("sort_column", ""),
            ("sort_descending", "false"),
        ])
        .await;
    assert_eq!(updated.status_code(), StatusCode::OK, "{}", updated.text());
    assert!(updated.text().contains(updated_email), "{}", updated.text());

    let deleted = fixture
        .client
        .server()
        .delete(&format!("{rows_path}?key_0={customer_id}"))
        .add_header("cookie", cookie.clone())
        .add_header("x-csrf-token", csrf.clone())
        .add_header("hx-request", "true")
        .await;
    assert_eq!(deleted.status_code(), StatusCode::OK, "{}", deleted.text());
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM public.customers WHERE id = $1")
        .bind(customer_id.parse::<i64>()?)
        .fetch_one(&engine_pool)
        .await?;
    assert_eq!(remaining, 0);

    let read_only_rows_path = format!("/c/{}/t/public/customers/rows", read_only_profile_id.0);
    let read_only_rows = fixture
        .client
        .server()
        .get(&read_only_rows_path)
        .add_header("cookie", cookie.clone())
        .add_header("hx-request", "true")
        .await;
    assert_eq!(
        read_only_rows.status_code(),
        StatusCode::OK,
        "{}",
        read_only_rows.text()
    );
    assert!(
        !read_only_rows.text().contains("Insert row"),
        "{}",
        read_only_rows.text()
    );
    let refused = fixture
        .client
        .server()
        .post(&read_only_rows_path)
        .add_header("cookie", cookie)
        .add_header("x-csrf-token", csrf)
        .add_header("hx-request", "true")
        .form(&[("value_1", "Must be refused")])
        .await;
    assert_eq!(
        refused.status_code(),
        StatusCode::CONFLICT,
        "{}",
        refused.text()
    );

    engine_pool.close().await;
    Ok(())
}

async fn save_profile(fixture: &TestApp, read_only: bool) -> TestResult<ConnectionProfileId> {
    let Authentication::Password { password } = &fixture.engine.connection.auth else {
        return Err(
            std::io::Error::other("PostgreSQL testkit requires password authentication").into(),
        );
    };
    let profile_id = ConnectionProfileId(Uuid::new_v4());
    let profile = ConnectionProfile::new(
        profile_id.clone(),
        Some(LOCAL_OWNER.to_owned()),
        "Schema HTTP test".to_owned(),
        Engine::Postgres,
        fixture.engine.connection.host.clone(),
        u32::from(fixture.engine.connection.port),
        fixture.engine.connection.database.clone(),
        fixture.engine.connection.user.clone(),
        ConnectionProfileAuth::Password {
            encrypted_secret: Some(EncryptedSecret::new(format!("test:{password}"))),
        },
        SslMode::Disable,
        None,
        read_only,
        None,
        None,
    )?;
    fixture.profiles.save(&profile).await?;
    Ok(profile_id)
}
