//! Catalog queries shared by PostgreSQL-shaped engines.

use std::{
    collections::{BTreeMap, HashSet},
    fmt::Write as _,
};

use sqlx::{FromRow, PgConnection, Row};

use crate::domain::database::{
    CompletionSchema, Constraint, ConstraintKind, DbError, DbErrorKind, Engine, ForeignKey, Index,
    RelationKind, RelationRef, RelationSummary, Relationship, RoutineKind, RoutineSummary,
    SchemaNode, SchemaTree, TableDetail,
};

const SCHEMAS_SQL: &str = "SELECT n.nspname FROM pg_namespace n \
    WHERE n.nspname NOT IN ('information_schema', 'crdb_internal', 'sys', 'system') \
      AND LEFT(n.nspname, 3) <> 'pg_' AND LEFT(n.nspname, 3) <> 'yb_' \
    ORDER BY n.nspname";

const RELATIONS_SQL: &str = "SELECT n.nspname AS schema, c.relname AS name, c.relkind::text AS relkind, \
    CASE WHEN c.relkind IN ('r', 'p', 'v', 'm', 'f') AND (c.reltuples IS NULL OR c.reltuples < 0 \
         OR ($1 AND c.reltuples = 0)) THEN NULL ELSE c.reltuples::bigint END AS row_estimate, \
    obj_description(c.oid, 'pg_class') AS comment \
    FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
    WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f', 'S') \
      AND n.nspname NOT IN ('information_schema', 'crdb_internal', 'sys', 'system') \
      AND LEFT(n.nspname, 3) <> 'pg_' AND LEFT(n.nspname, 3) <> 'yb_' \
    ORDER BY n.nspname, c.relname";

const ROUTINES_SQL: &str = "SELECT n.nspname AS schema, p.proname AS name, p.prokind::text AS kind, \
    pg_get_function_identity_arguments(p.oid) AS arguments, pg_get_function_result(p.oid) AS return_type \
    FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
    WHERE n.nspname NOT IN ('information_schema', 'crdb_internal', 'sys', 'system') \
      AND LEFT(n.nspname, 3) <> 'pg_' AND LEFT(n.nspname, 3) <> 'yb_' \
    ORDER BY n.nspname, p.proname, arguments";

const TABLE_INFO_SQL: &str = "SELECT c.relkind::text AS relkind, \
    CASE WHEN c.reltuples IS NULL OR c.reltuples < 0 OR ($3 AND c.reltuples = 0) \
         THEN NULL ELSE c.reltuples::bigint END AS row_estimate, \
    obj_description(c.oid, 'pg_class') AS comment \
    FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
    WHERE n.nspname = $1 AND c.relname = $2 \
      AND c.relkind IN ('r', 'p', 'v', 'm', 'f')";

const COLUMNS_SQL: &str = "SELECT a.attname AS name, a.attnum::int4 AS ordinal, \
    format_type(a.atttypid, a.atttypmod) AS data_type, NOT a.attnotnull AS nullable, \
    pg_get_expr(d.adbin, d.adrelid) AS default_expr, a.attidentity::text AS identity_kind, \
    a.attgenerated::text AS generated_kind, \
    CASE WHEN a.attgenerated <> '' THEN pg_get_expr(d.adbin, d.adrelid) ELSE NULL END AS generated_expr, \
    col_description(c.oid, a.attnum) AS comment \
    FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid \
    JOIN pg_namespace n ON n.oid = c.relnamespace \
    LEFT JOIN pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum \
    WHERE n.nspname = $1 AND c.relname = $2 AND a.attnum > 0 AND NOT a.attisdropped \
    ORDER BY a.attnum";

const CONSTRAINTS_SQL: &str = "SELECT con.conname AS name, con.contype::text AS kind, \
    pg_get_constraintdef(con.oid) AS definition \
    FROM pg_constraint con JOIN pg_class c ON c.oid = con.conrelid \
    JOIN pg_namespace n ON n.oid = c.relnamespace \
    WHERE n.nspname = $1 AND c.relname = $2 ORDER BY con.conname";

const PRIMARY_KEY_SQL: &str = "SELECT a.attname FROM pg_constraint con \
    JOIN pg_class c ON c.oid = con.conrelid JOIN pg_namespace n ON n.oid = c.relnamespace \
    JOIN LATERAL unnest(con.conkey) WITH ORDINALITY AS key(attnum, ordinal) ON TRUE \
    JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = key.attnum \
    WHERE n.nspname = $1 AND c.relname = $2 AND con.contype = 'p' ORDER BY key.ordinal";

const FOREIGN_KEYS_SQL: &str = "SELECT con.oid::bigint AS constraint_id, con.conname AS name, \
    n.nspname AS from_schema, c.relname AS from_table, source_att.attname AS from_column, \
    rn.nspname AS to_schema, rc.relname AS to_table, target_att.attname AS to_column, \
    source_key.ord::bigint AS ordinal, \
    CASE con.confupdtype::text WHEN 'r' THEN 'RESTRICT' WHEN 'c' THEN 'CASCADE' \
         WHEN 'n' THEN 'SET NULL' WHEN 'd' THEN 'SET DEFAULT' ELSE 'NO ACTION' END AS on_update, \
    CASE con.confdeltype::text WHEN 'r' THEN 'RESTRICT' WHEN 'c' THEN 'CASCADE' \
         WHEN 'n' THEN 'SET NULL' WHEN 'd' THEN 'SET DEFAULT' ELSE 'NO ACTION' END AS on_delete \
    FROM pg_constraint con JOIN pg_class c ON c.oid = con.conrelid \
    JOIN pg_namespace n ON n.oid = c.relnamespace \
    JOIN pg_class rc ON rc.oid = con.confrelid JOIN pg_namespace rn ON rn.oid = rc.relnamespace \
    JOIN LATERAL unnest(con.conkey) WITH ORDINALITY AS source_key(attnum, ord) ON TRUE \
    JOIN pg_attribute source_att ON source_att.attrelid = c.oid AND source_att.attnum = source_key.attnum \
    JOIN LATERAL unnest(con.confkey) WITH ORDINALITY AS target_key(attnum, ord) \
         ON target_key.ord = source_key.ord \
    JOIN pg_attribute target_att ON target_att.attrelid = rc.oid AND target_att.attnum = target_key.attnum \
    WHERE con.contype = 'f' AND n.nspname = $1 AND ($2::text IS NULL OR c.relname = $2) \
    ORDER BY con.oid, source_key.ord";

const INDEXES_SQL: &str = "SELECT i.relname AS name, pg_get_indexdef(i.oid) AS definition, \
    pg_get_indexdef(i.oid, key.ordinal, true) AS column_definition, \
    x.indisunique AS is_unique, x.indisprimary AS is_primary, \
    EXISTS (SELECT 1 FROM pg_constraint con WHERE con.conindid = x.indexrelid) AS constraint_owned, \
    CASE WHEN x.indisvalid THEN NULL ELSE 'invalid' END::text AS status \
    FROM pg_index x JOIN pg_class c ON c.oid = x.indrelid \
    JOIN pg_namespace n ON n.oid = c.relnamespace JOIN pg_class i ON i.oid = x.indexrelid \
    JOIN LATERAL generate_series(1, x.indnatts) AS key(ordinal) ON TRUE \
    WHERE n.nspname = $1 AND c.relname = $2 ORDER BY i.relname, key.ordinal";

const DSQL_INDEXES_SQL: &str = "SELECT i.relname AS name, pg_get_indexdef(i.oid) AS definition, \
    pg_get_indexdef(i.oid, key.ordinal, true) AS column_definition, \
    x.indisunique AS is_unique, x.indisprimary AS is_primary, \
    EXISTS (SELECT 1 FROM pg_constraint con WHERE con.conindid = x.indexrelid) AS constraint_owned, \
    COALESCE((SELECT j.status FROM sys.jobs j WHERE j.job_type = 'INDEX_BUILD' \
              AND j.object_id = i.oid ORDER BY j.update_time DESC LIMIT 1), \
             CASE WHEN x.indisvalid THEN NULL ELSE 'invalid' END)::text AS status \
    FROM pg_index x JOIN pg_class c ON c.oid = x.indrelid \
    JOIN pg_namespace n ON n.oid = c.relnamespace JOIN pg_class i ON i.oid = x.indexrelid \
    JOIN LATERAL generate_series(1, x.indnatts) AS key(ordinal) ON TRUE \
    WHERE n.nspname = $1 AND c.relname = $2 ORDER BY i.relname, key.ordinal";

const COMPLETION_COLUMNS_SQL: &str = "SELECT n.nspname AS schema, c.relname AS table_name, \
    a.attname AS column_name FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid \
    JOIN pg_namespace n ON n.oid = c.relnamespace \
    WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') AND a.attnum > 0 AND NOT a.attisdropped \
      AND n.nspname NOT IN ('information_schema', 'crdb_internal', 'sys', 'system') \
      AND LEFT(n.nspname, 3) <> 'pg_' AND LEFT(n.nspname, 3) <> 'yb_' \
    ORDER BY n.nspname, c.relname, a.attnum";

#[derive(FromRow)]
struct RelationRow {
    schema: String,
    name: String,
    relkind: String,
    row_estimate: Option<i64>,
    comment: Option<String>,
}
#[derive(FromRow)]
struct RoutineRow {
    schema: String,
    name: String,
    kind: String,
    arguments: String,
    return_type: Option<String>,
}
#[derive(FromRow)]
struct RelationInfo {
    relkind: String,
    row_estimate: Option<i64>,
    comment: Option<String>,
}
#[derive(FromRow)]
struct ColumnRow {
    name: String,
    ordinal: i32,
    data_type: String,
    nullable: bool,
    default_expr: Option<String>,
    identity_kind: String,
    generated_kind: String,
    generated_expr: Option<String>,
    comment: Option<String>,
}
#[derive(FromRow)]
struct ConstraintRow {
    name: String,
    kind: String,
    definition: String,
}
#[derive(FromRow)]
struct ForeignKeyRow {
    constraint_id: i64,
    name: String,
    from_schema: String,
    from_table: String,
    from_column: String,
    to_schema: String,
    to_table: String,
    to_column: String,
    ordinal: i64,
    on_update: String,
    on_delete: String,
}
#[derive(FromRow)]
struct IndexRow {
    name: String,
    definition: String,
    column_definition: String,
    is_unique: bool,
    is_primary: bool,
    constraint_owned: bool,
    status: Option<String>,
}
#[derive(FromRow)]
struct CompletionColumnRow {
    schema: String,
    table_name: String,
    column_name: String,
}

struct ForeignKeyRecord {
    from_schema: String,
    from_table: String,
    foreign_key: ForeignKey,
}

struct IndexResults {
    indexes: Vec<Index>,
    ddl_definitions: Vec<(String, bool)>,
}

struct DdlParts<'a> {
    relation: &'a RelationRef,
    kind: RelationKind,
    columns: &'a [crate::domain::database::Column],
    column_rows: &'a [ColumnRow],
    constraints: &'a [Constraint],
    indexes: &'a [(String, bool)],
    comment: Option<&'a str>,
}

/// Lists user schemas, their relations, sequences, and routines.
///
/// # Errors
/// Returns a database error when a catalog query fails.
pub async fn schema_tree(conn: &mut PgConnection, engine: Engine) -> Result<SchemaTree, DbError> {
    let names: Vec<String> = sqlx::query_scalar(SCHEMAS_SQL)
        .fetch_all(&mut *conn)
        .await
        .map_err(|error| query_error(&error))?;
    let mut schemas = BTreeMap::new();
    for name in names {
        schemas.insert(
            name.clone(),
            SchemaNode {
                name,
                relations: Vec::new(),
                sequences: Vec::new(),
                routines: Vec::new(),
            },
        );
    }

    let unreliable_zero = matches!(engine, Engine::CockroachDb | Engine::YugabyteDb);
    let rows: Vec<RelationRow> = sqlx::query_as(RELATIONS_SQL)
        .bind(unreliable_zero)
        .fetch_all(&mut *conn)
        .await
        .map_err(|error| query_error(&error))?;
    for row in rows {
        let Some(schema) = schemas.get_mut(&row.schema) else {
            continue;
        };
        if row.relkind == "S" {
            schema.sequences.push(row.name);
        } else if let Some(kind) = relation_kind(&row.relkind) {
            schema.relations.push(RelationSummary {
                name: row.name,
                kind,
                row_estimate: row.row_estimate,
                comment: row.comment,
            });
        }
    }

    // DSQL does not expose pg_proc and does not support user-defined routines.
    if engine != Engine::AuroraDsql {
        let rows: Vec<RoutineRow> = sqlx::query_as(ROUTINES_SQL)
            .fetch_all(&mut *conn)
            .await
            .map_err(|error| query_error(&error))?;
        for row in rows {
            if let Some(schema) = schemas.get_mut(&row.schema) {
                schema.routines.push(RoutineSummary {
                    name: row.name,
                    kind: if row.kind == "p" {
                        RoutineKind::Procedure
                    } else {
                        RoutineKind::Function
                    },
                    arguments: row.arguments,
                    return_type: row.return_type,
                });
            }
        }
    }
    Ok(SchemaTree {
        schemas: schemas.into_values().collect(),
    })
}

/// Reads columns, constraints, indexes, comments, and reconstructed DDL for a relation.
///
/// # Errors
/// Returns a database error when the relation is absent or a catalog query fails.
pub async fn table_detail(
    conn: &mut PgConnection,
    relation: &RelationRef,
    engine: Engine,
) -> Result<TableDetail, DbError> {
    let unreliable_zero = matches!(engine, Engine::CockroachDb | Engine::YugabyteDb);
    let info: RelationInfo = sqlx::query_as(TABLE_INFO_SQL)
        .bind(&relation.schema)
        .bind(&relation.name)
        .bind(unreliable_zero)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|error| query_error(&error))?
        .ok_or_else(|| {
            DbError::new(
                DbErrorKind::Query,
                format!(
                    "Relation {}.{} was not found",
                    relation.schema, relation.name
                ),
            )
        })?;
    let kind = relation_kind(&info.relkind).ok_or_else(|| {
        DbError::new(
            DbErrorKind::Query,
            format!(
                "Unsupported relation kind for {}.{}",
                relation.schema, relation.name
            ),
        )
    })?;
    let rows: Vec<ColumnRow> = sqlx::query_as(COLUMNS_SQL)
        .bind(&relation.schema)
        .bind(&relation.name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|error| query_error(&error))?;
    let columns: Vec<_> = rows
        .iter()
        .map(|row| {
            let generated = match row.identity_kind.as_str() {
                "a" => Some("GENERATED ALWAYS AS IDENTITY".to_owned()),
                "d" => Some("GENERATED BY DEFAULT AS IDENTITY".to_owned()),
                _ if !row.generated_kind.is_empty() => row.generated_expr.clone(),
                _ => None,
            };
            crate::domain::database::Column {
                name: row.name.clone(),
                ordinal: row.ordinal,
                data_type: row.data_type.clone(),
                nullable: row.nullable,
                default: row.default_expr.clone(),
                generated,
                comment: row.comment.clone(),
                hidden: false,
            }
        })
        .collect();

    let constraints = read_constraints(conn, relation).await?;
    let foreign_keys: Vec<ForeignKey> =
        read_foreign_keys(conn, &relation.schema, Some(&relation.name))
            .await?
            .into_iter()
            .map(|record| record.foreign_key)
            .collect();
    let IndexResults {
        indexes,
        ddl_definitions,
    } = read_indexes(conn, relation, engine).await?;
    let primary_key: Vec<String> = sqlx::query_scalar(PRIMARY_KEY_SQL)
        .bind(&relation.schema)
        .bind(&relation.name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|error| query_error(&error))?;
    let ddl = render_ddl(
        DdlParts {
            relation,
            kind,
            columns: &columns,
            column_rows: &rows,
            constraints: &constraints,
            indexes: &ddl_definitions,
            comment: info.comment.as_deref(),
        },
        conn,
    )
    .await?;
    Ok(TableDetail {
        relation: relation.clone(),
        kind,
        columns,
        primary_key,
        indexes,
        constraints,
        foreign_keys,
        row_estimate: info.row_estimate,
        comment: info.comment,
        ddl,
    })
}

/// Reads enforced foreign-key edges and, when appropriate, naming-based inferred edges.
///
/// # Errors
/// Returns a database error when catalog queries fail.
pub async fn relationships(
    conn: &mut PgConnection,
    schema: &str,
    engine: Engine,
) -> Result<Vec<Relationship>, DbError> {
    let foreign_keys = read_foreign_keys(conn, schema, None).await?;
    let should_infer = engine == Engine::AuroraDsql || foreign_keys.is_empty();
    let mut covered_columns = HashSet::new();
    let mut relationships = Vec::with_capacity(foreign_keys.len());
    for record in &foreign_keys {
        for column in &record.foreign_key.columns {
            covered_columns.insert((record.from_table.clone(), column.clone()));
        }
        relationships.push(Relationship {
            name: Some(record.foreign_key.name.clone()),
            from: RelationRef {
                schema: record.from_schema.clone(),
                name: record.from_table.clone(),
            },
            from_columns: record.foreign_key.columns.clone(),
            to: record.foreign_key.referenced.clone(),
            to_columns: record.foreign_key.referenced_columns.clone(),
            inferred: false,
        });
    }
    if should_infer {
        relationships.extend(infer_relationships(conn, schema, &covered_columns).await?);
    }
    relationships.sort_by(|left, right| {
        (
            &left.from.name,
            &left.from_columns,
            &left.to.name,
            &left.to_columns,
        )
            .cmp(&(
                &right.from.name,
                &right.from_columns,
                &right.to.name,
                &right.to_columns,
            ))
    });
    Ok(relationships)
}

/// Builds the schema/table/column map used by SQL completion.
///
/// # Errors
/// Returns a database error when catalog queries fail.
pub async fn completion_schema(conn: &mut PgConnection) -> Result<CompletionSchema, DbError> {
    let default_schema: Option<String> = sqlx::query_scalar("SELECT current_schema()")
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| query_error(&error))?;
    let rows: Vec<CompletionColumnRow> = sqlx::query_as(COMPLETION_COLUMNS_SQL)
        .fetch_all(&mut *conn)
        .await
        .map_err(|error| query_error(&error))?;
    let mut schemas = BTreeMap::new();
    for row in rows {
        schemas
            .entry(row.schema)
            .or_insert_with(BTreeMap::new)
            .entry(row.table_name)
            .or_insert_with(Vec::new)
            .push(row.column_name);
    }
    Ok(CompletionSchema {
        default_schema: default_schema.unwrap_or_else(|| "public".to_owned()),
        schemas,
    })
}

/// Reads `CockroachDB`'s native table DDL.
///
/// # Errors
/// Returns a database error when `SHOW CREATE TABLE` fails.
pub async fn cockroach_create_table(
    conn: &mut PgConnection,
    relation: &RelationRef,
) -> Result<String, DbError> {
    // SHOW CREATE TABLE requires an identifier rather than bind parameters. Quote both components.
    let sql = format!(
        "SHOW CREATE TABLE {}.{}",
        quote_identifier(&relation.schema),
        quote_identifier(&relation.name)
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .fetch_optional(&mut *conn)
        .await
        .map_err(|error| query_error(&error))?
        .ok_or_else(|| DbError::new(DbErrorKind::Query, "SHOW CREATE TABLE returned no row"))?;
    row.try_get::<String, _>(1)
        .map_err(|error| query_error(&error))
}

pub fn mark_cockroach_hidden_rowid(detail: &mut TableDetail) {
    let ddl = detail.ddl.to_ascii_lowercase();
    if ddl.contains("not visible")
        && ddl.contains("unique_rowid()")
        && let Some(rowid) = detail
            .columns
            .iter_mut()
            .find(|column| column.name == "rowid")
    {
        rowid.hidden = true;
    }
}

async fn read_constraints(
    conn: &mut PgConnection,
    relation: &RelationRef,
) -> Result<Vec<Constraint>, DbError> {
    let rows: Vec<ConstraintRow> = sqlx::query_as(CONSTRAINTS_SQL)
        .bind(&relation.schema)
        .bind(&relation.name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|error| query_error(&error))?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let kind = match row.kind.as_str() {
                "p" => ConstraintKind::PrimaryKey,
                "u" => ConstraintKind::Unique,
                "c" => ConstraintKind::Check,
                "f" => ConstraintKind::ForeignKey,
                "x" => ConstraintKind::Exclusion,
                _ => return None,
            };
            Some(Constraint {
                name: row.name,
                kind,
                definition: row.definition,
            })
        })
        .collect())
}

async fn read_foreign_keys(
    conn: &mut PgConnection,
    schema: &str,
    table: Option<&str>,
) -> Result<Vec<ForeignKeyRecord>, DbError> {
    let mut rows: Vec<ForeignKeyRow> = sqlx::query_as(FOREIGN_KEYS_SQL)
        .bind(schema)
        .bind(table)
        .fetch_all(&mut *conn)
        .await
        .map_err(|error| query_error(&error))?;
    rows.sort_by_key(|row| (row.constraint_id, row.ordinal));
    let mut grouped = BTreeMap::new();
    for row in rows {
        let record = grouped
            .entry(row.constraint_id)
            .or_insert_with(|| ForeignKeyRecord {
                from_schema: row.from_schema.clone(),
                from_table: row.from_table.clone(),
                foreign_key: ForeignKey {
                    name: row.name.clone(),
                    columns: Vec::new(),
                    referenced: RelationRef {
                        schema: row.to_schema.clone(),
                        name: row.to_table.clone(),
                    },
                    referenced_columns: Vec::new(),
                    on_update: row.on_update.clone(),
                    on_delete: row.on_delete.clone(),
                },
            });
        record.foreign_key.columns.push(row.from_column);
        record.foreign_key.referenced_columns.push(row.to_column);
    }
    Ok(grouped.into_values().collect())
}

async fn read_indexes(
    conn: &mut PgConnection,
    relation: &RelationRef,
    engine: Engine,
) -> Result<IndexResults, DbError> {
    let rows: Vec<IndexRow> = if engine == Engine::AuroraDsql {
        match sqlx::query_as(DSQL_INDEXES_SQL)
            .bind(&relation.schema)
            .bind(&relation.name)
            .fetch_all(&mut *conn)
            .await
        {
            Ok(rows) => rows,
            // Async-job catalogs are optional in DSQL-compatible implementations.
            Err(_) => sqlx::query_as(INDEXES_SQL)
                .bind(&relation.schema)
                .bind(&relation.name)
                .fetch_all(&mut *conn)
                .await
                .map_err(|error| query_error(&error))?,
        }
    } else {
        sqlx::query_as(INDEXES_SQL)
            .bind(&relation.schema)
            .bind(&relation.name)
            .fetch_all(&mut *conn)
            .await
            .map_err(|error| query_error(&error))?
    };
    let mut indexes = BTreeMap::<String, (Index, bool)>::new();
    for row in rows {
        let (index, constraint_owned) = indexes.entry(row.name.clone()).or_insert_with(|| {
            (
                Index {
                    name: row.name.clone(),
                    definition: row.definition.clone(),
                    columns: Vec::new(),
                    unique: row.is_unique,
                    primary: row.is_primary,
                    status: row.status.clone(),
                },
                row.constraint_owned,
            )
        });
        index.columns.push(row.column_definition);
        if index.status.is_none() {
            index.status = row.status;
        }
        *constraint_owned |= row.constraint_owned;
    }
    let definitions = indexes
        .values()
        .filter(|(index, owned)| !index.primary && !owned)
        .map(|(index, _)| (index.definition.clone(), engine == Engine::AuroraDsql))
        .collect();
    Ok(IndexResults {
        indexes: indexes.into_values().map(|(index, _)| index).collect(),
        ddl_definitions: definitions,
    })
}

async fn render_ddl(parts: DdlParts<'_>, conn: &mut PgConnection) -> Result<String, DbError> {
    let DdlParts {
        relation,
        kind,
        columns,
        column_rows,
        constraints,
        indexes,
        comment,
    } = parts;
    let qualified = format!(
        "{}.{}",
        quote_identifier(&relation.schema),
        quote_identifier(&relation.name)
    );
    let mut ddl = if matches!(kind, RelationKind::View | RelationKind::MaterializedView) {
        let definition: Option<String> = sqlx::query_scalar(
            "SELECT pg_get_viewdef(c.oid, true) FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2",
        )
        .bind(&relation.schema)
        .bind(&relation.name)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|error| query_error(&error))?;
        let definition = definition.ok_or_else(|| {
            DbError::new(
                DbErrorKind::Query,
                "View definition was not found in the catalog",
            )
        })?;
        let create_kind = if kind == RelationKind::MaterializedView {
            "CREATE MATERIALIZED VIEW"
        } else {
            "CREATE VIEW"
        };
        format!("{create_kind} {qualified} AS\n{definition};")
    } else {
        let mut definitions = Vec::with_capacity(columns.len() + constraints.len());
        for (column, row) in columns.iter().zip(column_rows) {
            definitions.push(render_column_definition(column, &row.generated_kind));
        }
        let mut ordered: Vec<_> = constraints.iter().collect();
        ordered.sort_by(|left, right| left.name.cmp(&right.name));
        definitions.extend(ordered.into_iter().map(|constraint| {
            format!(
                "CONSTRAINT {} {}",
                quote_identifier(&constraint.name),
                constraint.definition
            )
        }));
        let mut statement = format!(
            "CREATE TABLE {qualified} (\n    {}\n);",
            definitions.join(",\n    ")
        );
        for (definition, asynchronous) in indexes {
            statement.push_str("\n\n");
            statement.push_str(&render_index_definition(definition, *asynchronous));
            statement.push(';');
        }
        statement
    };
    if let Some(comment) = comment {
        ddl.push_str("\n\n");
        let _ = write!(
            &mut ddl,
            "COMMENT ON {} {qualified} IS {};",
            comment_kind(kind),
            quote_literal(comment)
        );
    }
    for column in columns {
        if let Some(comment) = &column.comment {
            ddl.push_str("\n\n");
            let _ = write!(
                &mut ddl,
                "COMMENT ON COLUMN {qualified}.{} IS {};",
                quote_identifier(&column.name),
                quote_literal(comment)
            );
        }
    }
    Ok(ddl)
}

fn render_column_definition(
    column: &crate::domain::database::Column,
    generated_kind: &str,
) -> String {
    let mut definition = format!("{} {}", quote_identifier(&column.name), column.data_type);
    match column.generated.as_deref() {
        Some("GENERATED ALWAYS AS IDENTITY") => {
            definition.push_str(" GENERATED ALWAYS AS IDENTITY");
        }
        Some("GENERATED BY DEFAULT AS IDENTITY") => {
            definition.push_str(" GENERATED BY DEFAULT AS IDENTITY");
        }
        Some(expression) => {
            let storage = if generated_kind == "v" {
                "VIRTUAL"
            } else {
                "STORED"
            };
            let _ = write!(
                &mut definition,
                " GENERATED ALWAYS AS ({expression}) {storage}"
            );
        }
        None => {
            if let Some(default) = &column.default {
                let _ = write!(&mut definition, " DEFAULT {default}");
            }
        }
    }
    if !column.nullable {
        definition.push_str(" NOT NULL");
    }
    definition
}

fn render_index_definition(definition: &str, asynchronous: bool) -> String {
    if !asynchronous {
        return definition.to_owned();
    }
    definition
        .replacen("CREATE UNIQUE INDEX ", "CREATE UNIQUE INDEX ASYNC ", 1)
        .replacen("CREATE INDEX ", "CREATE INDEX ASYNC ", 1)
}

fn comment_kind(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::View => "VIEW",
        RelationKind::MaterializedView => "MATERIALIZED VIEW",
        _ => "TABLE",
    }
}

async fn infer_relationships(
    conn: &mut PgConnection,
    schema: &str,
    covered: &HashSet<(String, String)>,
) -> Result<Vec<Relationship>, DbError> {
    let tables: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.relname, a.attname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_attribute a ON a.attrelid = c.oid WHERE n.nspname = $1 AND c.relkind IN ('r', 'p') \
         AND a.attnum > 0 AND NOT a.attisdropped ORDER BY c.relname, a.attnum"
    ).bind(schema).fetch_all(&mut *conn).await.map_err(|error| query_error(&error))?;
    let primary_tables: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relkind IN ('r', 'p') AND EXISTS ( \
         SELECT 1 FROM pg_constraint con JOIN pg_attribute a ON a.attrelid = c.oid AND a.attname = 'id' \
         WHERE con.conrelid = c.oid AND con.contype = 'p' AND array_length(con.conkey, 1) = 1 \
         AND con.conkey[1] = a.attnum) ORDER BY c.relname"
    ).bind(schema).fetch_all(&mut *conn).await.map_err(|error| query_error(&error))?;
    let primary_tables: HashSet<_> = primary_tables.into_iter().collect();
    let table_columns: BTreeMap<String, Vec<String>> =
        tables
            .into_iter()
            .fold(BTreeMap::new(), |mut map, (table, column)| {
                map.entry(table).or_default().push(column);
                map
            });
    let mut inferred = Vec::new();
    for (from_table, columns) in &table_columns {
        for column in columns {
            if covered.contains(&(from_table.clone(), column.clone())) {
                continue;
            }
            let Some(stem) = column.strip_suffix("_id") else {
                continue;
            };
            let Some(to_table) = table_variants(stem)
                .into_iter()
                .find(|table| primary_tables.contains(table))
            else {
                continue;
            };
            inferred.push(Relationship {
                name: None,
                from: RelationRef {
                    schema: schema.to_owned(),
                    name: from_table.clone(),
                },
                from_columns: vec![column.clone()],
                to: RelationRef {
                    schema: schema.to_owned(),
                    name: to_table,
                },
                to_columns: vec!["id".to_owned()],
                inferred: true,
            });
        }
    }
    Ok(inferred)
}

fn table_variants(stem: &str) -> Vec<String> {
    let plural = if let Some(root) = stem.strip_suffix('y') {
        if root
            .chars()
            .last()
            .is_some_and(|character| !"aeiou".contains(character))
        {
            format!("{root}ies")
        } else {
            format!("{stem}s")
        }
    } else if stem.ends_with('s')
        || stem.ends_with('x')
        || stem.ends_with("ch")
        || stem.ends_with("sh")
    {
        format!("{stem}es")
    } else {
        format!("{stem}s")
    };
    vec![stem.to_owned(), plural]
}

fn relation_kind(relkind: &str) -> Option<RelationKind> {
    match relkind {
        "r" => Some(RelationKind::Table),
        "p" => Some(RelationKind::PartitionedTable),
        "v" => Some(RelationKind::View),
        "m" => Some(RelationKind::MaterializedView),
        "f" => Some(RelationKind::ForeignTable),
        _ => None,
    }
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}
fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn query_error(error: &sqlx::Error) -> DbError {
    DbError::new(DbErrorKind::Query, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{quote_identifier, quote_literal, table_variants};

    #[test]
    fn quotes_catalog_identifiers_and_comments() {
        assert_eq!(quote_identifier("a\"b"), "\"a\"\"b\"");
        assert_eq!(quote_literal("it's"), "'it''s'");
    }

    #[test]
    fn expands_common_singular_names_to_plural_tables() {
        assert_eq!(table_variants("category"), ["category", "categories"]);
        assert_eq!(table_variants("status"), ["status", "statuses"]);
        assert_eq!(table_variants("customer"), ["customer", "customers"]);
    }
}
