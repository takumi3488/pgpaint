use super::engine::Engine;
use super::sql::{
    StatementClass, Token, TokenKind, after_group, classify_statement, command_index,
    is_identifier, split_statements, tokenize,
};

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::module_name_repetitions)]
pub struct LintIssue {
    pub severity: LintSeverity,
    pub rule: &'static str,
    pub message: String,
    pub statement_index: usize,
    pub offset: usize,
}

/// Reports blocking compatibility/read-only issues and non-blocking safety warnings.
///
/// The linter is intentionally lexical and conservative: malformed statements are skipped
/// rather than guessed at, and it does not claim to validate general SQL grammar.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn lint(engine: Engine, sql: &str, read_only: bool) -> Vec<LintIssue> {
    let spans = split_statements(sql);
    let mut issues = Vec::new();
    let mut in_transaction = false;
    let mut ddl_seen = false;
    let mut data_modified = false;
    let mut first_outside_transaction = None;

    for span in &spans {
        let Some(tokens) = tokenize(span.text) else {
            continue;
        };
        let Some(command) = command_index(&tokens, 0) else {
            continue;
        };
        let statement_class = classify_statement(&tokens, 0, true);
        let operation = operation_index(&tokens, command);
        let outside_transaction = !in_transaction
            && statement_class != StatementClass::Transaction
            && statement_class != StatementClass::Session;

        if outside_transaction {
            first_outside_transaction.get_or_insert((span.index, span.start_offset));
        }

        if read_only && matches!(statement_class, StatementClass::Dml | StatementClass::Ddl) {
            push_issue(
                &mut issues,
                LintSeverity::Error,
                "read-only-profile",
                "This statement modifies data or schema, but the connection is read-only.",
                span.index,
                span.start_offset,
            );
        }

        if let Some(index) = operation {
            let token = &tokens[index];
            if (token.is_word("UPDATE") || token.is_word("DELETE"))
                && !has_top_level_word(&tokens, index, "WHERE")
            {
                push_issue(
                    &mut issues,
                    LintSeverity::Warning,
                    "write-without-where",
                    "UPDATE or DELETE has no top-level WHERE clause and may affect every row.",
                    span.index,
                    offset_of(span.start_offset, Some(token)),
                );
            }
            if token.is_word("DROP") || token.is_word("TRUNCATE") {
                push_issue(
                    &mut issues,
                    LintSeverity::Warning,
                    "destructive-ddl",
                    "DROP or TRUNCATE is destructive; verify the target before execution.",
                    span.index,
                    offset_of(span.start_offset, Some(token)),
                );
            }
        }
        lint_cte_write_without_where(span.index, span.start_offset, &tokens, &mut issues);

        if engine == Engine::AuroraDsql {
            lint_dsql_statement(
                span.index,
                span.start_offset,
                &tokens,
                command,
                operation,
                &mut issues,
            );
        }

        // Aurora DSQL permits one DDL operation per transaction and forbids mixing DDL
        // with DML: https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-postgresql-compatibility-migration-guide.html
        // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-ddl.html
        if engine == Engine::AuroraDsql && in_transaction {
            match statement_class {
                StatementClass::Ddl => {
                    if ddl_seen {
                        push_issue(
                            &mut issues,
                            LintSeverity::Error,
                            "dsql-one-ddl-per-transaction",
                            "Aurora DSQL allows at most one DDL statement in an explicit transaction.",
                            span.index,
                            span.start_offset,
                        );
                    }
                    if data_modified {
                        push_issue(
                            &mut issues,
                            LintSeverity::Error,
                            "dsql-no-ddl-dml-mix",
                            "Aurora DSQL does not allow DDL and DML in the same transaction.",
                            span.index,
                            span.start_offset,
                        );
                    }
                    ddl_seen = true;
                }
                StatementClass::Dml => {
                    if ddl_seen {
                        push_issue(
                            &mut issues,
                            LintSeverity::Error,
                            "dsql-no-ddl-dml-mix",
                            "Aurora DSQL does not allow DDL and DML in the same transaction.",
                            span.index,
                            span.start_offset,
                        );
                    }
                    data_modified = true;
                }
                _ => {}
            }
        }
        if engine == Engine::CockroachDb {
            lint_cockroachdb_extension(
                span.index,
                span.start_offset,
                &tokens,
                command,
                &mut issues,
            );
        }

        if let Some(index) = operation {
            let token = &tokens[index];
            if token.is_word("BEGIN") || token.is_word("START") {
                in_transaction = true;
            } else if (token.is_word("COMMIT")
                || token.is_word("END")
                || token.is_word("ROLLBACK")
                || token.is_word("ABORT"))
                && !contains_word_after(&tokens, index, "TO")
            {
                let chained = tokens
                    .iter()
                    .enumerate()
                    .skip(index + 1)
                    .any(|(next, token)| {
                        token.is_word("CHAIN")
                            && !tokens
                                .get(next - 1)
                                .is_some_and(|previous| previous.is_word("NO"))
                    });
                in_transaction = chained;
                ddl_seen = false;
                data_modified = false;
            }
        }
    }

    // Aurora DSQL is PostgreSQL-compatible, so standalone statements follow PostgreSQL autocommit:
    // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with.html
    // https://www.postgresql.org/docs/current/sql-begin.html
    if let Some((statement_index, offset)) =
        first_outside_transaction.filter(|_| engine == Engine::AuroraDsql && spans.len() > 1)
    {
        push_issue(
            &mut issues,
            LintSeverity::Warning,
            "dsql-standalone-statements",
            "Statements outside an explicit BEGIN..COMMIT block run in separate transactions on Aurora DSQL.",
            statement_index,
            offset,
        );
    }

    issues
}

fn operation_index(tokens: &[Token<'_>], command: usize) -> Option<usize> {
    let token = tokens.get(command)?;
    if !token.is_word("EXPLAIN") {
        return Some(command);
    }
    if !explain_has_analyze(tokens, command) {
        return None;
    }
    let mut target = command + 1;
    if tokens.get(target).is_some_and(|token| token.is_symbol("(")) {
        target = after_group(tokens, target)?;
    }
    while tokens.get(target).is_some_and(|token| {
        token.is_word("ANALYZE")
            || token.is_word("VERBOSE")
            || token.is_word("COSTS")
            || token.is_word("BUFFERS")
            || token.is_word("TIMING")
            || token.is_word("SUMMARY")
            || token.is_word("SETTINGS")
            || token.is_word("WAL")
            || token.is_word("FORMAT")
    }) {
        target += 1;
        if tokens.get(target).is_some_and(|token| {
            token.is_word("TRUE")
                || token.is_word("FALSE")
                || token.is_word("JSON")
                || token.is_word("TEXT")
                || token.is_word("XML")
                || token.is_word("YAML")
        }) {
            target += 1;
        }
    }
    command_index(tokens, target)
}

fn explain_has_analyze(tokens: &[Token<'_>], explain: usize) -> bool {
    let index = explain + 1;
    if tokens.get(index).is_some_and(|token| token.is_symbol("(")) {
        let Some(end) = after_group(tokens, index) else {
            return false;
        };
        let options = &tokens[index + 1..end - 1];
        if let Some(analyze) = options.iter().position(|token| token.is_word("ANALYZE")) {
            return !options
                .get(analyze + 1)
                .is_some_and(|token| token.is_word("FALSE"));
        }
        return tokens
            .get(end)
            .is_some_and(|token| token.is_word("ANALYZE"));
    }
    tokens
        .get(index)
        .is_some_and(|token| token.is_word("ANALYZE"))
}

fn has_top_level_word(tokens: &[Token<'_>], start: usize, word: &str) -> bool {
    let mut depth = 0_usize;
    for token in tokens.iter().skip(start) {
        if token.is_symbol("(") {
            depth += 1;
        } else if token.is_symbol(")") {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && token.is_word(word) {
            return true;
        }
    }
    false
}

fn has_top_level_word_before(tokens: &[Token<'_>], start: usize, end: usize, word: &str) -> bool {
    let mut depth = 0_usize;
    for token in tokens.iter().take(end).skip(start) {
        if token.is_symbol("(") {
            depth += 1;
        } else if token.is_symbol(")") {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && token.is_word(word) {
            return true;
        }
    }
    false
}

fn lint_cte_write_without_where(
    statement_index: usize,
    base_offset: usize,
    tokens: &[Token<'_>],
    issues: &mut Vec<LintIssue>,
) {
    if !tokens.first().is_some_and(|token| token.is_word("WITH")) {
        return;
    }
    let mut index = 1;
    if tokens
        .get(index)
        .is_some_and(|token| token.is_word("RECURSIVE"))
    {
        index += 1;
    }
    loop {
        if !tokens.get(index).is_some_and(is_identifier) {
            return;
        }
        index += 1;
        if tokens.get(index).is_some_and(|token| token.is_symbol("(")) {
            let Some(after_columns) = after_group(tokens, index) else {
                return;
            };
            index = after_columns;
        }
        if !tokens.get(index).is_some_and(|token| token.is_word("AS")) {
            return;
        }
        index += 1;
        if tokens.get(index).is_some_and(|token| token.is_word("NOT")) {
            index += 1;
            if !tokens
                .get(index)
                .is_some_and(|token| token.is_word("MATERIALIZED"))
            {
                return;
            }
            index += 1;
        } else if tokens
            .get(index)
            .is_some_and(|token| token.is_word("MATERIALIZED"))
        {
            index += 1;
        }
        if !tokens.get(index).is_some_and(|token| token.is_symbol("(")) {
            return;
        }
        let body_start = index + 1;
        let Some(after_body) = after_group(tokens, index) else {
            return;
        };
        let command = command_index(tokens, body_start).filter(|&command| {
            tokens
                .get(command)
                .is_some_and(|token| token.is_word("UPDATE") || token.is_word("DELETE"))
                && !has_top_level_word_before(tokens, command, after_body - 1, "WHERE")
        });
        if let Some(command) = command {
            let token = &tokens[command];
            push_issue(
                issues,
                LintSeverity::Warning,
                "write-without-where",
                "UPDATE or DELETE has no top-level WHERE clause and may affect every row.",
                statement_index,
                offset_of(base_offset, Some(token)),
            );
        }
        index = after_body;
        if tokens.get(index).is_some_and(|token| token.is_symbol(",")) {
            index += 1;
        } else {
            return;
        }
    }
}

fn has_top_level_sequence(
    tokens: &[Token<'_>],
    start: usize,
    end: usize,
    first: &str,
    second: &str,
) -> bool {
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().take(end).skip(start) {
        if token.is_symbol("(") {
            depth += 1;
        } else if token.is_symbol(")") {
            depth = depth.saturating_sub(1);
        } else if depth == 0
            && token.is_word(first)
            && tokens
                .get(index + 1)
                .is_some_and(|next| next.is_word(second))
        {
            return true;
        }
    }
    false
}

fn contains_word_in_range(tokens: &[Token<'_>], start: usize, end: usize, word: &str) -> bool {
    tokens
        .iter()
        .take(end)
        .skip(start)
        .any(|token| token.is_word(word))
}

fn alter_actions_start(tokens: &[Token<'_>], command: usize) -> usize {
    let mut index = command + 2;
    if tokens
        .get(index)
        .is_some_and(|token| token.is_word("ASYNC"))
    {
        index += 1;
    }
    if tokens.get(index).is_some_and(|token| token.is_word("IF"))
        && tokens
            .get(index + 1)
            .is_some_and(|token| token.is_word("EXISTS"))
    {
        index += 2;
    }
    if tokens.get(index).is_some_and(|token| token.is_word("ONLY")) {
        index += 1;
    }
    if tokens
        .get(index)
        .is_some_and(|token| matches!(token.kind, TokenKind::Word | TokenKind::QuotedIdentifier))
    {
        index += 1;
        while tokens.get(index).is_some_and(|token| token.is_symbol(".")) {
            index += 1;
            if tokens.get(index).is_some_and(|token| {
                matches!(token.kind, TokenKind::Word | TokenKind::QuotedIdentifier)
            }) {
                index += 1;
            }
        }
        if tokens.get(index).is_some_and(|token| token.is_symbol("*")) {
            index += 1;
        }
    }
    index
}

fn alter_action_end(tokens: &[Token<'_>], start: usize) -> usize {
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.is_symbol("(") {
            depth += 1;
        } else if token.is_symbol(")") {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && token.is_symbol(",") {
            return index;
        }
    }
    tokens.len()
}

fn contains_word_after(tokens: &[Token<'_>], start: usize, word: &str) -> bool {
    tokens
        .iter()
        .skip(start + 1)
        .any(|token| token.is_word(word))
}

fn push_issue(
    issues: &mut Vec<LintIssue>,
    severity: LintSeverity,
    rule: &'static str,
    message: &str,
    statement_index: usize,
    offset: usize,
) {
    issues.push(LintIssue {
        severity,
        rule,
        message: message.to_owned(),
        statement_index,
        offset,
    });
}

fn offset_of(base: usize, token: Option<&Token<'_>>) -> usize {
    token.map_or(base, |token| base + token.start)
}

#[allow(clippy::too_many_lines)]
fn lint_dsql_statement(
    statement_index: usize,
    base_offset: usize,
    tokens: &[Token<'_>],
    command: usize,
    operation: Option<usize>,
    issues: &mut Vec<LintIssue>,
) {
    let Some(token) = tokens.get(command) else {
        return;
    };
    let current = operation.unwrap_or(command);

    // The compatibility command matrix and migration guide document unsupported SQL,
    // triggers, temporary tables, and non-SQL procedural languages:
    // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-postgresql-compatibility-supported-sql-features.html
    // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-postgresql-compatibility-migration-guide.html
    if token.is_word("CREATE") {
        let table_index = create_table_index(tokens, command);
        if tokens
            .get(command + 1)
            .is_some_and(|next| next.is_word("EXTENSION"))
        {
            issue_at_token(
                issues,
                LintSeverity::Error,
                "dsql-create-extension",
                "Aurora DSQL does not support CREATE EXTENSION.",
                statement_index,
                base_offset,
                tokens.get(command + 1),
            );
        }
        if is_create_trigger(tokens, command) || is_create_constraint_trigger(tokens, command) {
            issue_at_token(
                issues,
                LintSeverity::Error,
                "dsql-triggers",
                "Aurora DSQL does not support triggers; move trigger logic into the application.",
                statement_index,
                base_offset,
                find_word(tokens, command, "TRIGGER"),
            );
        }
        // CREATE TYPE is not supported in the DSQL command matrix or custom-type docs:
        // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-postgresql-compatibility-supported-sql-features.html
        // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-postgresql-compatibility-supported-data-types.html
        if tokens
            .get(command + 1)
            .is_some_and(|next| next.is_word("TYPE"))
        {
            issue_at_token(
                issues,
                LintSeverity::Error,
                "dsql-create-type",
                "Aurora DSQL does not support user-defined composite types.",
                statement_index,
                base_offset,
                tokens.get(command + 1),
            );
        }
        let table_index = table_index.filter(|&table_index| {
            tokens[command + 1..table_index].iter().any(|candidate| {
                candidate.is_word("TEMP")
                    || candidate.is_word("TEMPORARY")
                    || candidate.is_word("UNLOGGED")
            })
        });
        if let Some(table_index) = table_index {
            issue_at_token(
                issues,
                LintSeverity::Error,
                "dsql-table-persistence",
                "Aurora DSQL does not support TEMP, TEMPORARY, or UNLOGGED tables.",
                statement_index,
                base_offset,
                tokens.get(table_index),
            );
        }
        let language = if is_function_or_procedure(tokens, command) {
            language_clause(tokens, command).filter(|language| !language.is_word("SQL"))
        } else {
            None
        };
        if let Some(language) = language {
            issue_at_token(
                issues,
                LintSeverity::Error,
                "dsql-non-sql-language",
                "Aurora DSQL supports SQL-language routines only; procedural languages such as PL/pgSQL are unsupported.",
                statement_index,
                base_offset,
                Some(language),
            );
        }
        let create_index = tokens
            .get(command + 1)
            .is_some_and(|next| next.is_word("INDEX"))
            || (tokens
                .get(command + 1)
                .is_some_and(|next| next.is_word("UNIQUE"))
                && tokens
                    .get(command + 2)
                    .is_some_and(|next| next.is_word("INDEX")));
        let index_keyword = if create_index {
            find_word_index(tokens, command, "INDEX")
        } else {
            None
        };
        let missing_async_index = index_keyword
            .and_then(|index| {
                find_word_index(tokens, index + 1, "ON").map(|on| {
                    let has_async = tokens[index..on]
                        .iter()
                        .any(|candidate| candidate.is_word("ASYNC"));
                    (index, !has_async)
                })
            })
            .filter(|(_, missing_async)| *missing_async)
            .map(|(index, _)| index);
        if let Some(index_keyword) = missing_async_index {
            issue_at_token(
                issues,
                LintSeverity::Error,
                "dsql-index-async",
                "Aurora DSQL index creation is asynchronous; add ASYNC after INDEX.",
                statement_index,
                base_offset,
                tokens.get(index_keyword),
            );
        }
    }
    if operation.is_some_and(|index| tokens[index].is_word("DO")) {
        issue_at_token(
            issues,
            LintSeverity::Error,
            "dsql-non-sql-language",
            "Aurora DSQL does not support anonymous procedural-language blocks.",
            statement_index,
            base_offset,
            operation.and_then(|index| tokens.get(index)),
        );
    }

    // Aurora DSQL's supported type matrix marks arrays as query-runtime-only; the
    // documented persistent data types are listed here:
    // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-postgresql-compatibility-supported-data-types.html
    // SERIAL pseudo-types are not among the supported CREATE TABLE data types; use the
    // documented identity-column syntax instead:
    // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/create-table-syntax-support.html
    // Foreign keys are intentionally allowed because current DSQL CREATE TABLE syntax supports them:
    // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/create-table-syntax-support.html
    if is_create_table(tokens, command) || is_alter_table(tokens, command) {
        lint_column_types(statement_index, base_offset, tokens, command, issues);
    }

    // CREATE INDEX ASYNC is the required form documented by Aurora DSQL:
    // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/create-index-syntax-support.html
    // (The detailed check is in the CREATE branch above.)

    // Only the listed ALTER TABLE forms are supported, and constraint validation must be ASYNC:
    // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/alter-table-syntax-support.html
    if is_alter_table(tokens, command) {
        lint_alter_table(statement_index, base_offset, tokens, command, issues);
    }

    // Managed maintenance and pessimistic locking/listener commands are not supported;
    // TRUNCATE is explicitly replaced by DELETE in the migration guide:
    // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-postgresql-compatibility-supported-sql-features.html
    // https://docs.aws.amazon.com/aurora-dsql/latest/userguide/working-with-postgresql-compatibility-migration-guide.html
    if operation.is_some_and(|index| {
        ["VACUUM", "LOCK", "LISTEN", "NOTIFY", "UNLISTEN", "TRUNCATE"]
            .iter()
            .any(|word| tokens[index].is_word(word))
    }) {
        issue_at_token(
            issues,
            LintSeverity::Error,
            "dsql-unsupported-utility",
            "This utility command is unsupported by Aurora DSQL; use the documented DSQL-compatible alternative when available.",
            statement_index,
            base_offset,
            tokens.get(current),
        );
    }
}

fn lint_cockroachdb_extension(
    statement_index: usize,
    base_offset: usize,
    tokens: &[Token<'_>],
    command: usize,
    issues: &mut Vec<LintIssue>,
) {
    // CockroachDB's official feature matrix explicitly rejects these extension names:
    // https://www.cockroachlabs.com/docs/stable/sql-feature-support.html
    if !tokens
        .get(command)
        .is_some_and(|token| token.is_word("CREATE"))
        || !tokens
            .get(command + 1)
            .is_some_and(|token| token.is_word("EXTENSION"))
    {
        return;
    }
    let mut extension_index = command + 2;
    if tokens
        .get(extension_index)
        .is_some_and(|token| token.is_word("IF"))
        && tokens
            .get(extension_index + 1)
            .is_some_and(|token| token.is_word("NOT"))
        && tokens
            .get(extension_index + 2)
            .is_some_and(|token| token.is_word("EXISTS"))
    {
        extension_index += 3;
    }
    let Some(extension) = tokens.get(extension_index) else {
        return;
    };
    let unsupported = [
        "citext",
        "btree_gin",
        "btree_gist",
        "postgres_fdw",
        "pg_stat_statements",
    ];
    let name = if extension.kind == TokenKind::QuotedIdentifier {
        extension
            .text
            .strip_prefix('"')
            .and_then(|name| name.strip_suffix('"'))
    } else if extension.kind == TokenKind::Word {
        Some(extension.text)
    } else {
        None
    };
    let is_unsupported = match name {
        Some(name) if extension.kind == TokenKind::QuotedIdentifier => unsupported.contains(&name),
        Some(name) => unsupported
            .iter()
            .any(|unsupported| name.eq_ignore_ascii_case(unsupported)),
        None => false,
    };
    if is_unsupported {
        issue_at_token(
            issues,
            LintSeverity::Error,
            "crdb-unsupported-extension",
            "This PostgreSQL extension is explicitly unsupported by CockroachDB.",
            statement_index,
            base_offset,
            Some(extension),
        );
    }
}

fn is_create_trigger(tokens: &[Token<'_>], command: usize) -> bool {
    let mut index = command + 1;
    if tokens.get(index).is_some_and(|token| token.is_word("OR"))
        && tokens
            .get(index + 1)
            .is_some_and(|token| token.is_word("REPLACE"))
    {
        index += 2;
    }
    tokens
        .get(index)
        .is_some_and(|token| token.is_word("TRIGGER"))
}

fn is_create_constraint_trigger(tokens: &[Token<'_>], command: usize) -> bool {
    let mut index = command + 1;
    if tokens.get(index).is_some_and(|token| token.is_word("OR"))
        && tokens
            .get(index + 1)
            .is_some_and(|token| token.is_word("REPLACE"))
    {
        index += 2;
    }
    tokens
        .get(index)
        .is_some_and(|token| token.is_word("CONSTRAINT"))
        && tokens
            .get(index + 1)
            .is_some_and(|token| token.is_word("TRIGGER"))
}

fn is_function_or_procedure(tokens: &[Token<'_>], command: usize) -> bool {
    let mut index = command + 1;
    if tokens.get(index).is_some_and(|token| token.is_word("OR"))
        && tokens
            .get(index + 1)
            .is_some_and(|token| token.is_word("REPLACE"))
    {
        index += 2;
    }
    tokens
        .get(index)
        .is_some_and(|token| token.is_word("FUNCTION") || token.is_word("PROCEDURE"))
}

fn language_clause<'tokens, 'sql>(
    tokens: &'tokens [Token<'sql>],
    command: usize,
) -> Option<&'tokens Token<'sql>> {
    tokens
        .iter()
        .enumerate()
        .skip(command + 1)
        .rev()
        .find(|(_, token)| token.is_word("LANGUAGE"))
        .and_then(|(index, _)| tokens.get(index + 1))
}

fn find_word<'text, 'tokens>(
    tokens: &'tokens [Token<'text>],
    start: usize,
    word: &str,
) -> Option<&'tokens Token<'text>> {
    tokens.iter().skip(start).find(|token| token.is_word(word))
}

fn issue_at_token(
    issues: &mut Vec<LintIssue>,
    severity: LintSeverity,
    rule: &'static str,
    message: &str,
    statement_index: usize,
    base_offset: usize,
    token: Option<&Token<'_>>,
) {
    push_issue(
        issues,
        severity,
        rule,
        message,
        statement_index,
        offset_of(base_offset, token),
    );
}

fn is_create_table(tokens: &[Token<'_>], command: usize) -> bool {
    create_table_index(tokens, command).is_some()
}

fn create_table_index(tokens: &[Token<'_>], command: usize) -> Option<usize> {
    if !tokens
        .get(command)
        .is_some_and(|token| token.is_word("CREATE"))
    {
        return None;
    }
    let mut index = command + 1;
    if tokens.get(index).is_some_and(|token| token.is_word("OR"))
        && tokens
            .get(index + 1)
            .is_some_and(|token| token.is_word("REPLACE"))
    {
        index += 2;
    }
    if tokens
        .get(index)
        .is_some_and(|token| token.is_word("GLOBAL") || token.is_word("LOCAL"))
    {
        index += 1;
    }
    if tokens.get(index).is_some_and(|token| {
        token.is_word("TEMP") || token.is_word("TEMPORARY") || token.is_word("UNLOGGED")
    }) {
        index += 1;
    }
    tokens
        .get(index)
        .is_some_and(|token| token.is_word("TABLE"))
        .then_some(index)
}
fn is_alter_table(tokens: &[Token<'_>], command: usize) -> bool {
    tokens
        .get(command)
        .is_some_and(|token| token.is_word("ALTER"))
        && tokens
            .get(command + 1)
            .is_some_and(|token| token.is_word("TABLE"))
}

#[allow(clippy::too_many_lines)]
fn lint_column_types(
    statement_index: usize,
    base_offset: usize,
    tokens: &[Token<'_>],
    command: usize,
    issues: &mut Vec<LintIssue>,
) {
    const UNSUPPORTED_TYPES: &[&str] = &[
        "INET",
        "CIDR",
        "MACADDR",
        "MONEY",
        "XML",
        "POINT",
        "TSVECTOR",
        "TSQUERY",
        "SERIAL",
        "BIGSERIAL",
        "SMALLSERIAL",
    ];
    let mut candidates = Vec::new();

    if is_create_table(tokens, command) {
        let Some(table_index) = create_table_index(tokens, command) else {
            return;
        };
        let Some(open) = tokens
            .iter()
            .enumerate()
            .skip(table_index + 1)
            .find(|(_, token)| token.is_symbol("("))
            .map(|(index, _)| index)
        else {
            return;
        };
        let Some(end) = after_group(tokens, open) else {
            return;
        };
        let mut segment_start = open + 1;
        let mut depth = 0_usize;
        for (index, token) in tokens.iter().enumerate().take(end - 1).skip(open + 1) {
            if token.is_symbol("(") {
                depth += 1;
            } else if token.is_symbol(")") {
                depth = depth.saturating_sub(1);
            } else if depth == 0 && token.is_symbol(",") {
                column_type_at(
                    tokens,
                    segment_start,
                    index,
                    UNSUPPORTED_TYPES,
                    &mut candidates,
                );
                segment_start = index + 1;
            }
        }
        column_type_at(
            tokens,
            segment_start,
            end - 1,
            UNSUPPORTED_TYPES,
            &mut candidates,
        );
    } else if is_alter_table(tokens, command) {
        for (index, token) in tokens
            .iter()
            .enumerate()
            .skip(alter_actions_start(tokens, command))
        {
            if !token.is_word("ADD") {
                continue;
            }
            let mut column = index + 1;
            if tokens
                .get(column)
                .is_some_and(|token| token.is_word("COLUMN"))
            {
                column += 1;
            }
            if tokens.get(column).is_some_and(|token| token.is_word("IF"))
                && tokens
                    .get(column + 1)
                    .is_some_and(|token| token.is_word("NOT"))
                && tokens
                    .get(column + 2)
                    .is_some_and(|token| token.is_word("EXISTS"))
            {
                column += 3;
            }
            let type_index = column + 1;
            let actual_type = if tokens
                .get(type_index + 1)
                .is_some_and(|token| token.is_symbol("."))
            {
                type_index + 2
            } else {
                type_index
            };
            let array_type = tokens
                .get(actual_type + 1)
                .is_some_and(|token| token.is_symbol("[") || token.is_word("ARRAY"));
            if array_type
                || tokens.get(actual_type).is_some_and(|token| {
                    token.kind == TokenKind::Word
                        && UNSUPPORTED_TYPES
                            .iter()
                            .any(|type_name| token.text.eq_ignore_ascii_case(type_name))
                })
            {
                candidates.push(actual_type);
            }
        }
    }

    for index in candidates {
        let serial = tokens.get(index).is_some_and(|token| {
            ["SERIAL", "BIGSERIAL", "SMALLSERIAL"]
                .iter()
                .any(|type_name| token.text.eq_ignore_ascii_case(type_name))
        });
        let (rule, message) = if serial {
            (
                "dsql-serial-types",
                "Aurora DSQL does not support SERIAL pseudo-types; use a supported identity column or sequence.",
            )
        } else {
            (
                "dsql-unsupported-column-type",
                "This PostgreSQL type is not supported as a persistent Aurora DSQL column type.",
            )
        };
        issue_at_token(
            issues,
            LintSeverity::Error,
            rule,
            message,
            statement_index,
            base_offset,
            tokens.get(index),
        );
    }
}

fn column_type_at(
    tokens: &[Token<'_>],
    start: usize,
    end: usize,
    unsupported: &[&str],
    candidates: &mut Vec<usize>,
) {
    let Some(first) = tokens.get(start) else {
        return;
    };
    if [
        "CONSTRAINT",
        "PRIMARY",
        "UNIQUE",
        "CHECK",
        "FOREIGN",
        "EXCLUDE",
    ]
    .iter()
    .any(|word| first.is_word(word))
    {
        return;
    }
    let mut type_index = start + 1;
    if type_index + 2 < end
        && tokens[type_index + 1].is_symbol(".")
        && tokens[type_index + 2].kind == TokenKind::Word
    {
        type_index += 2;
    }
    let array_type = tokens
        .get(type_index + 1)
        .is_some_and(|token| token.is_symbol("[") || token.is_word("ARRAY"));
    if array_type
        || tokens.get(type_index).is_some_and(|token| {
            token.kind == TokenKind::Word
                && unsupported
                    .iter()
                    .any(|type_name| token.text.eq_ignore_ascii_case(type_name))
        })
    {
        candidates.push(type_index);
    }
}

fn find_word_index(tokens: &[Token<'_>], start: usize, word: &str) -> Option<usize> {
    tokens
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, token)| token.is_word(word))
        .map(|(index, _)| index)
}

#[allow(clippy::too_many_lines)]
fn lint_alter_table(
    statement_index: usize,
    base_offset: usize,
    tokens: &[Token<'_>],
    command: usize,
    issues: &mut Vec<LintIssue>,
) {
    let has_async = tokens
        .get(command + 2)
        .is_some_and(|token| token.is_word("ASYNC"));
    let mut invalid_validation = None;
    let mut invalid_add_constraint = None;
    let mut invalid_add_unique = None;
    let mut invalid_type_change = None;

    for (index, token) in tokens
        .iter()
        .enumerate()
        .skip(alter_actions_start(tokens, command))
    {
        let action_end = alter_action_end(tokens, index);
        if token.is_word("VALIDATE")
            && tokens
                .get(index + 1)
                .is_some_and(|token| token.is_word("CONSTRAINT"))
            && !has_async
        {
            invalid_validation = Some(index);
        }
        if token.is_word("ADD") {
            let constraint_form = tokens.get(index + 1).is_some_and(|token| {
                token.is_word("CHECK") || token.is_word("FOREIGN") || token.is_word("CONSTRAINT")
            });
            let unique = contains_word_in_range(tokens, index, action_end, "UNIQUE");
            let primary_key = contains_word_in_range(tokens, index, action_end, "PRIMARY");
            let unique_using_index = unique
                && contains_word_in_range(tokens, index, action_end, "USING")
                && contains_word_in_range(tokens, index, action_end, "INDEX");
            if (unique || primary_key) && !unique_using_index {
                invalid_add_unique = Some(index);
            } else if constraint_form
                && !has_top_level_sequence(tokens, index, action_end, "NOT", "VALID")
                && !unique_using_index
            {
                invalid_add_constraint = Some(index);
            }
        }
        let after_column = index + 3;
        let changes_type = token.is_word("ALTER")
            && tokens
                .get(index + 1)
                .is_some_and(|token| token.is_word("COLUMN"))
            && (tokens
                .get(after_column)
                .is_some_and(|token| token.is_word("TYPE"))
                || (tokens
                    .get(after_column)
                    .is_some_and(|token| token.is_word("SET"))
                    && tokens
                        .get(after_column + 1)
                        .is_some_and(|token| token.is_word("DATA"))
                    && tokens
                        .get(after_column + 2)
                        .is_some_and(|token| token.is_word("TYPE"))));
        if changes_type {
            invalid_type_change = Some(index);
        }
    }

    if let Some(index) = invalid_validation {
        issue_at_token(
            issues,
            LintSeverity::Error,
            "dsql-alter-validate-async",
            "Aurora DSQL constraint validation requires ALTER TABLE ASYNC ... VALIDATE CONSTRAINT.",
            statement_index,
            base_offset,
            tokens.get(index),
        );
    }
    if let Some(index) = invalid_add_constraint {
        issue_at_token(
            issues,
            LintSeverity::Error,
            "dsql-alter-add-constraint-not-valid",
            "Aurora DSQL requires ADD CHECK/FOREIGN KEY constraints to use NOT VALID.",
            statement_index,
            base_offset,
            tokens.get(index),
        );
    }
    if let Some(index) = invalid_add_unique {
        issue_at_token(
            issues,
            LintSeverity::Error,
            "dsql-alter-add-unique-using-index",
            "Aurora DSQL only supports adding a UNIQUE constraint with USING INDEX.",
            statement_index,
            base_offset,
            tokens.get(index),
        );
    }
    if let Some(index) = invalid_type_change {
        issue_at_token(
            issues,
            LintSeverity::Error,
            "dsql-alter-column-type",
            "Aurora DSQL does not support ALTER TABLE ALTER COLUMN ... TYPE.",
            statement_index,
            base_offset,
            tokens.get(index),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{LintSeverity, lint};
    use crate::domain::database::sql::classify;
    use crate::domain::database::{Engine, StatementClass};

    fn has_rule(engine: Engine, sql: &str, read_only: bool, rule: &str) -> bool {
        lint(engine, sql, read_only)
            .iter()
            .any(|issue| issue.rule == rule)
    }

    #[test]
    fn generic_write_without_where_rule_has_positive_and_negative_cases() {
        let cases = [
            ("UPDATE t SET x = 1", true),
            (
                "WITH changed AS (UPDATE t SET x = 1 RETURNING id) SELECT id FROM changed",
                true,
            ),
            (
                "WITH changed AS (UPDATE t SET x = 1 WHERE id = 1 RETURNING id) SELECT id FROM changed",
                false,
            ),
            ("UPDATE t SET x = 1 WHERE id = 1", false),
            ("DELETE FROM t", true),
            ("DELETE FROM t WHERE id = 1", false),
            ("UPDATE t SET x = (SELECT x FROM u WHERE id = 1)", true),
        ];
        for (sql, expected) in cases {
            assert_eq!(
                has_rule(Engine::Postgres, sql, false, "write-without-where"),
                expected,
                "{sql}"
            );
        }
    }

    #[test]
    fn generic_destructive_ddl_rule_has_positive_and_negative_cases() {
        let cases = [
            ("DROP TABLE t", true),
            ("TRUNCATE t", true),
            ("SELECT 'DROP TABLE t'", false),
        ];
        for (sql, expected) in cases {
            assert_eq!(
                has_rule(Engine::Postgres, sql, false, "destructive-ddl"),
                expected,
                "{sql}"
            );
        }
    }

    #[test]
    fn generic_read_only_profile_rule_has_positive_and_negative_cases() {
        let cases = [
            ("UPDATE t SET x = 1", true),
            (
                "WITH removed AS (DELETE FROM t RETURNING id) SELECT id FROM removed",
                true,
            ),
            ("CREATE TABLE t (id int)", true),
            ("SELECT 1", false),
            ("BEGIN", false),
        ];
        for (sql, expected) in cases {
            assert_eq!(
                has_rule(Engine::Postgres, sql, true, "read-only-profile"),
                expected,
                "{sql}"
            );
        }
    }

    #[test]
    fn dsql_unsupported_creation_rules_have_positive_and_negative_cases() {
        let cases = [
            ("CREATE EXTENSION pgcrypto", "dsql-create-extension", true),
            ("CREATE TABLE t (id int)", "dsql-create-extension", false),
            (
                "CREATE TRIGGER tr BEFORE INSERT ON t EXECUTE FUNCTION f()",
                "dsql-triggers",
                true,
            ),
            (
                "CREATE CONSTRAINT TRIGGER tr AFTER INSERT ON t EXECUTE FUNCTION f()",
                "dsql-triggers",
                true,
            ),
            (
                "CREATE FUNCTION f() RETURNS int LANGUAGE plpgsql AS $$ SELECT 1 $$",
                "dsql-non-sql-language",
                true,
            ),
            (
                "CREATE FUNCTION f() RETURNS int LANGUAGE SQL AS $$ SELECT 1 $$",
                "dsql-non-sql-language",
                false,
            ),
            (
                "DO $$ BEGIN PERFORM 1; END $$",
                "dsql-non-sql-language",
                true,
            ),
            ("SELECT 1", "dsql-non-sql-language", false),
            (
                "CREATE TEMP TABLE t (id int)",
                "dsql-table-persistence",
                true,
            ),
            (
                "CREATE TEMPORARY TABLE t (id int)",
                "dsql-table-persistence",
                true,
            ),
            (
                "CREATE UNLOGGED TABLE t (id int)",
                "dsql-table-persistence",
                true,
            ),
            (
                "CREATE TABLE t (temporary int)",
                "dsql-table-persistence",
                false,
            ),
            ("CREATE TYPE mood AS ENUM ('ok')", "dsql-create-type", true),
            ("CREATE TABLE t (id int)", "dsql-create-type", false),
            ("CREATE INDEX ix ON t (id)", "dsql-index-async", true),
            (
                "CREATE UNIQUE INDEX ASYNC ix ON t (id)",
                "dsql-index-async",
                false,
            ),
            ("CREATE INDEX ASYNC ix ON t (id)", "dsql-index-async", false),
        ];
        for (sql, rule, expected) in cases {
            assert_eq!(
                has_rule(Engine::AuroraDsql, sql, false, rule),
                expected,
                "{sql}: {rule}"
            );
        }
    }

    #[test]
    fn dsql_serial_and_unsupported_column_type_rules_have_positive_and_negative_cases() {
        let cases = [
            (
                "CREATE TABLE t (id SERIAL PRIMARY KEY)",
                "dsql-serial-types",
                true,
            ),
            ("CREATE TABLE t (id BIGSERIAL)", "dsql-serial-types", true),
            ("CREATE TABLE t (id SMALLSERIAL)", "dsql-serial-types", true),
            (
                "CREATE TABLE serial (serial int)",
                "dsql-serial-types",
                false,
            ),
            (
                "CREATE TABLE t (id BIGINT GENERATED BY DEFAULT AS IDENTITY)",
                "dsql-serial-types",
                false,
            ),
            (
                "CREATE TABLE t (ip inet)",
                "dsql-unsupported-column-type",
                true,
            ),
            (
                "ALTER TABLE t ADD COLUMN ip inet",
                "dsql-unsupported-column-type",
                true,
            ),
            (
                "CREATE TABLE t (items integer[])",
                "dsql-unsupported-column-type",
                true,
            ),
            (
                "CREATE TABLE t (items int ARRAY)",
                "dsql-unsupported-column-type",
                true,
            ),
            (
                "CREATE TABLE t (data jsonb)",
                "dsql-unsupported-column-type",
                false,
            ),
            (
                "CREATE TABLE t (parent_id int REFERENCES parent(id))",
                "dsql-unsupported-column-type",
                false,
            ),
        ];
        for (sql, rule, expected) in cases {
            assert_eq!(
                has_rule(Engine::AuroraDsql, sql, false, rule),
                expected,
                "{sql}: {rule}"
            );
        }
        assert!(
            lint(
                Engine::AuroraDsql,
                "CREATE TABLE child (parent_id int REFERENCES parent(id))",
                false,
            )
            .is_empty(),
            "the current Aurora DSQL CREATE TABLE reference lists foreign keys as supported",
        );
    }

    #[test]
    fn dsql_unsupported_utility_rules_have_positive_and_negative_cases() {
        let cases = [
            ("VACUUM t", "dsql-unsupported-utility", true),
            ("LOCK TABLE t", "dsql-unsupported-utility", true),
            ("LISTEN channel", "dsql-unsupported-utility", true),
            ("NOTIFY channel", "dsql-unsupported-utility", true),
            ("SELECT 'VACUUM; LISTEN'", "dsql-unsupported-utility", false),
            ("TRUNCATE TABLE t", "dsql-unsupported-utility", true),
        ];
        for (sql, rule, expected) in cases {
            assert_eq!(
                has_rule(Engine::AuroraDsql, sql, false, rule),
                expected,
                "{sql}: {rule}"
            );
        }
    }

    #[test]
    fn cockroachdb_rejects_only_extensions_the_official_matrix_marks_unsupported() {
        let cases = [
            (Engine::CockroachDb, "CREATE EXTENSION citext", true),
            (
                Engine::CockroachDb,
                "CREATE EXTENSION IF NOT EXISTS \"btree_gin\"",
                true,
            ),
            (Engine::CockroachDb, "CREATE EXTENSION btree_gist", true),
            (Engine::CockroachDb, "CREATE EXTENSION postgres_fdw", true),
            (
                Engine::CockroachDb,
                "CREATE EXTENSION pg_stat_statements",
                true,
            ),
            (Engine::CockroachDb, "CREATE EXTENSION pgcrypto", false),
            (Engine::CockroachDb, "CREATE EXTENSION postgis", false),
            (Engine::Postgres, "CREATE EXTENSION citext", false),
            (Engine::CockroachDb, "CREATE EXTENSION \"CITEXT\"", false),
            (Engine::YugabyteDb, "CREATE EXTENSION citext", false),
            (
                Engine::CockroachDb,
                "SELECT 'CREATE EXTENSION citext'",
                false,
            ),
        ];
        for (engine, sql, expected) in cases {
            assert_eq!(
                has_rule(engine, sql, false, "crdb-unsupported-extension"),
                expected,
                "{sql}"
            );
        }
    }

    #[test]
    fn dsql_alter_table_rules_have_positive_and_negative_cases() {
        let cases = [
            (
                "ALTER TABLE t ALTER COLUMN id TYPE bigint",
                "dsql-alter-column-type",
                true,
            ),
            (
                "ALTER TABLE t ALTER COLUMN id SET DATA TYPE bigint",
                "dsql-alter-column-type",
                true,
            ),
            (
                "ALTER TABLE t ALTER COLUMN id SET DEFAULT 1",
                "dsql-alter-column-type",
                false,
            ),
            (
                "ALTER TABLE t ADD CONSTRAINT fk FOREIGN KEY (id) REFERENCES p(id)",
                "dsql-alter-add-constraint-not-valid",
                true,
            ),
            (
                "ALTER TABLE t ADD FOREIGN KEY (id) REFERENCES p(id) NOT VALID",
                "dsql-alter-add-constraint-not-valid",
                false,
            ),
            (
                "ALTER TABLE t ADD CONSTRAINT ck CHECK (id > 0) NOT VALID",
                "dsql-alter-add-constraint-not-valid",
                false,
            ),
            (
                "ALTER TABLE t ADD UNIQUE (id)",
                "dsql-alter-add-unique-using-index",
                true,
            ),
            (
                "ALTER TABLE t ADD UNIQUE USING INDEX ix",
                "dsql-alter-add-unique-using-index",
                false,
            ),
            (
                "ALTER TABLE ASYNC t VALIDATE CONSTRAINT fk",
                "dsql-alter-validate-async",
                false,
            ),
            (
                "ALTER TABLE t VALIDATE CONSTRAINT fk",
                "dsql-alter-validate-async",
                true,
            ),
            (
                "ALTER TABLE t ADD COLUMN label text",
                "dsql-alter-add-constraint-not-valid",
                false,
            ),
        ];
        for (sql, rule, expected) in cases {
            assert_eq!(
                has_rule(Engine::AuroraDsql, sql, false, rule),
                expected,
                "{sql}: {rule}"
            );
        }
    }

    #[test]
    fn dsql_transaction_and_batch_rules_have_positive_and_negative_cases() {
        let cases = [
            (
                "BEGIN; CREATE TABLE a (id int); CREATE TABLE b (id int); COMMIT",
                "dsql-one-ddl-per-transaction",
                true,
            ),
            (
                "BEGIN; CREATE TABLE a (id int); COMMIT",
                "dsql-one-ddl-per-transaction",
                false,
            ),
            (
                "BEGIN; CREATE TABLE a (id int); INSERT INTO a VALUES (1); COMMIT",
                "dsql-no-ddl-dml-mix",
                true,
            ),
            (
                "BEGIN; INSERT INTO a VALUES (1); CREATE TABLE b (id int); COMMIT",
                "dsql-no-ddl-dml-mix",
                true,
            ),
            (
                "BEGIN; INSERT INTO a VALUES (1); COMMIT",
                "dsql-no-ddl-dml-mix",
                false,
            ),
            ("SELECT 1; SELECT 2", "dsql-standalone-statements", true),
            ("SELECT 1", "dsql-standalone-statements", false),
            (
                "BEGIN; SELECT 1; COMMIT",
                "dsql-standalone-statements",
                false,
            ),
            (
                "BEGIN; SELECT 1; COMMIT; SELECT 2",
                "dsql-standalone-statements",
                true,
            ),
        ];
        for (sql, rule, expected) in cases {
            assert_eq!(
                has_rule(Engine::AuroraDsql, sql, false, rule),
                expected,
                "{sql}: {rule}"
            );
        }
    }

    #[test]
    fn issues_carry_byte_locations_and_accurate_severity() {
        let sql = "SELECT 1;\n  UPDATE t SET x = 1";
        let issues = lint(Engine::Postgres, sql, false);
        let issue = issues
            .iter()
            .find(|issue| issue.rule == "write-without-where");
        assert!(issue.is_some(), "UPDATE without WHERE should be warned");
        if let Some(issue) = issue {
            assert_eq!(issue.statement_index, 1);
            assert_eq!(&sql[issue.offset..issue.offset + 6], "UPDATE");
            assert_eq!(issue.severity, LintSeverity::Warning);
        }
        let errors = lint(Engine::AuroraDsql, "CREATE INDEX ix ON t (id)", false);
        assert_eq!(errors[0].severity, LintSeverity::Error);
    }

    #[test]
    fn malformed_input_is_skipped_without_panicking() {
        assert_eq!(
            lint(Engine::AuroraDsql, "CREATE TABLE t (id int", false),
            Vec::new(),
            "malformed CREATE TABLE should be skipped",
        );
        assert_eq!(
            lint(
                Engine::AuroraDsql,
                "SELECT 'unterminated; DROP TABLE t",
                false
            ),
            Vec::new(),
            "malformed string input should be skipped",
        );
    }

    #[test]
    fn explain_analyze_is_classified_as_the_explained_command() {
        assert_eq!(classify("EXPLAIN SELECT 1"), StatementClass::Query);
        assert_eq!(
            classify("EXPLAIN ANALYZE UPDATE t SET x = 1"),
            StatementClass::Dml
        );
        assert!(!has_rule(
            Engine::Postgres,
            "EXPLAIN UPDATE t SET x = 1",
            false,
            "write-without-where"
        ));
        assert!(has_rule(
            Engine::Postgres,
            "EXPLAIN ANALYZE UPDATE t SET x = 1",
            false,
            "write-without-where"
        ));
    }
}
