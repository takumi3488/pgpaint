/// A non-empty SQL statement and its location in the original input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatementSpan<'a> {
    pub text: &'a str,
    /// Byte offset of the first non-whitespace byte in `text`.
    pub start_offset: usize,
    /// Zero-based position among non-empty statements.
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementClass {
    Query,
    Dml,
    Ddl,
    Transaction,
    Session,
    Other,
}

/// Splits a SQL batch at semicolons outside strings, quoted identifiers, and comments.
///
/// This is a lexical splitter, not a PostgreSQL parser. Unterminated lexical constructs
/// consume the remainder of the input so malformed SQL is never split inside a literal.
#[must_use]
pub fn split_statements(sql: &str) -> Vec<StatementSpan<'_>> {
    let bytes = sql.as_bytes();
    let mut spans = Vec::new();
    let mut statement_start = 0;
    let mut cursor = 0;
    let mut has_code = false;

    while cursor < bytes.len() {
        if bytes[cursor..].starts_with(b"--") {
            cursor = skip_line_comment(bytes, cursor + 2);
            continue;
        }
        if bytes[cursor..].starts_with(b"/*") {
            cursor = skip_block_comment(bytes, cursor).unwrap_or(bytes.len());
            continue;
        }
        if bytes[cursor] == b'\'' {
            has_code = true;
            let escape = escape_string_starts_at(bytes, cursor);
            cursor = skip_quoted(sql, cursor, b'\'', escape).unwrap_or(bytes.len());
            continue;
        }
        if bytes[cursor] == b'"' {
            has_code = true;
            cursor = skip_quoted(sql, cursor, b'"', false).unwrap_or(bytes.len());
            continue;
        }
        let delimiter = if bytes[cursor] == b'$' {
            dollar_delimiter(sql, cursor)
        } else {
            None
        };
        if let Some(delimiter) = delimiter {
            has_code = true;
            let body_start = cursor + delimiter.len();
            cursor = sql[body_start..]
                .find(delimiter)
                .map_or(bytes.len(), |relative_end| {
                    body_start + relative_end + delimiter.len()
                });
            continue;
        }
        if bytes[cursor] == b';' {
            push_span(sql, statement_start, cursor, has_code, &mut spans);
            statement_start = cursor + 1;
            cursor += 1;
            has_code = false;
            continue;
        }

        let Some(character) = sql
            .get(cursor..)
            .and_then(|remaining| remaining.chars().next())
        else {
            break;
        };
        if !character.is_whitespace() {
            has_code = true;
        }
        cursor += character.len_utf8();
    }

    push_span(sql, statement_start, bytes.len(), has_code, &mut spans);
    spans
}

fn push_span<'a>(
    sql: &'a str,
    start: usize,
    end: usize,
    has_code: bool,
    spans: &mut Vec<StatementSpan<'a>>,
) {
    if !has_code {
        return;
    }
    let raw = &sql[start..end];
    let leading_trimmed = raw.trim_start();
    let left_trim = raw.len() - leading_trimmed.len();
    let text = leading_trimmed.trim_end();
    if text.is_empty() {
        return;
    }
    spans.push(StatementSpan {
        text,
        start_offset: start + left_trim,
        index: spans.len(),
    });
}

fn skip_line_comment(bytes: &[u8], mut cursor: usize) -> usize {
    while cursor < bytes.len() && bytes[cursor] != b'\n' {
        cursor += 1;
    }
    cursor
}

fn skip_block_comment(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start + 2;
    let mut depth = 1_usize;
    while cursor < bytes.len() {
        if bytes[cursor..].starts_with(b"/*") {
            depth += 1;
            cursor += 2;
        } else if bytes[cursor..].starts_with(b"*/") {
            depth -= 1;
            cursor += 2;
            if depth == 0 {
                return Some(cursor);
            }
        } else {
            cursor += 1;
        }
    }
    None
}

fn skip_quoted(sql: &str, quote_at: usize, quote: u8, escape: bool) -> Option<usize> {
    let bytes = sql.as_bytes();
    let mut cursor = quote_at + 1;
    while cursor < bytes.len() {
        if escape && bytes[cursor] == b'\\' {
            cursor += 1;
            cursor += sql.get(cursor..)?.chars().next()?.len_utf8();
        } else if bytes[cursor] == quote && bytes.get(cursor + 1) == Some(&quote) {
            cursor += 2;
        } else if bytes[cursor] == quote {
            return Some(cursor + 1);
        } else {
            cursor += sql.get(cursor..)?.chars().next()?.len_utf8();
        }
    }
    None
}

fn escape_string_starts_at(bytes: &[u8], quote_at: usize) -> bool {
    quote_at > 0
        && matches!(bytes[quote_at - 1], b'e' | b'E')
        && (quote_at < 2 || !is_identifier_continue(bytes[quote_at - 2]))
}

fn dollar_delimiter(sql: &str, start: usize) -> Option<&str> {
    let bytes = sql.as_bytes();
    if start > 0 && is_identifier_continue(bytes[start - 1]) {
        return None;
    }
    if bytes.get(start) != Some(&b'$') {
        return None;
    }
    let mut cursor = start + 1;
    if bytes.get(cursor) == Some(&b'$') {
        cursor += 1;
        return Some(&sql[start..cursor]);
    }
    if !bytes.get(cursor).copied().is_some_and(is_identifier_start) {
        return None;
    }
    cursor += 1;
    while bytes
        .get(cursor)
        .copied()
        .is_some_and(is_dollar_tag_continue)
    {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'$') {
        return None;
    }
    cursor += 1;
    Some(&sql[start..cursor])
}

const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

const fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80
}

const fn is_dollar_tag_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenKind {
    Word,
    QuotedIdentifier,
    Literal,
    Symbol,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Token<'a> {
    pub(super) text: &'a str,
    pub(super) start: usize,
    pub(super) kind: TokenKind,
}

impl Token<'_> {
    pub(super) fn is_word(self, word: &str) -> bool {
        self.kind == TokenKind::Word && self.text.eq_ignore_ascii_case(word)
    }

    pub(super) fn is_symbol(self, symbol: &str) -> bool {
        self.kind == TokenKind::Symbol && self.text == symbol
    }
}

pub(super) fn tokenize(sql: &str) -> Option<Vec<Token<'_>>> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
            continue;
        }
        if bytes[cursor..].starts_with(b"--") {
            cursor = skip_line_comment(bytes, cursor + 2);
            continue;
        }
        if bytes[cursor..].starts_with(b"/*") {
            cursor = skip_block_comment(bytes, cursor)?;
            continue;
        }
        if bytes[cursor] == b'\'' {
            let escape = escape_string_starts_at(bytes, cursor);
            let start = if escape { cursor - 1 } else { cursor };
            if escape
                && tokens.last().is_some_and(|token: &Token<'_>| {
                    token.kind == TokenKind::Word
                        && token.start == start
                        && token.text.eq_ignore_ascii_case("E")
                })
            {
                tokens.pop();
            }
            cursor = skip_quoted(sql, cursor, b'\'', escape)?;
            tokens.push(Token {
                text: &sql[start..cursor],
                start,
                kind: TokenKind::Literal,
            });
            continue;
        }
        if bytes[cursor] == b'"' {
            let start = cursor;
            cursor = skip_quoted(sql, cursor, b'"', false)?;
            tokens.push(Token {
                text: &sql[start..cursor],
                start,
                kind: TokenKind::QuotedIdentifier,
            });
            continue;
        }
        let delimiter = if bytes[cursor] == b'$' {
            dollar_delimiter(sql, cursor)
        } else {
            None
        };
        if let Some(delimiter) = delimiter {
            let start = cursor;
            let body_start = cursor + delimiter.len();
            let relative_end = sql[body_start..].find(delimiter)?;
            cursor = body_start + relative_end + delimiter.len();
            tokens.push(Token {
                text: &sql[start..cursor],
                start,
                kind: TokenKind::Literal,
            });
            continue;
        }
        if is_identifier_start(bytes[cursor]) {
            let start = cursor;
            cursor += 1;
            while bytes
                .get(cursor)
                .copied()
                .is_some_and(is_identifier_continue)
            {
                cursor += 1;
            }
            tokens.push(Token {
                text: &sql[start..cursor],
                start,
                kind: TokenKind::Word,
            });
            continue;
        }

        let start = cursor;
        cursor += sql[cursor..].chars().next()?.len_utf8();
        tokens.push(Token {
            text: &sql[start..cursor],
            start,
            kind: TokenKind::Symbol,
        });
    }
    Some(tokens)
}

/// Classifies the leading operation in a statement, including the command after CTEs.
#[must_use]
pub fn classify(stmt: &str) -> StatementClass {
    let Some(tokens) = tokenize(stmt) else {
        return StatementClass::Other;
    };
    classify_statement(&tokens, 0, true)
}

pub(super) fn classify_statement(
    tokens: &[Token<'_>],
    start: usize,
    explain_allowed: bool,
) -> StatementClass {
    let Some(command) = command_index(tokens, start) else {
        return StatementClass::Other;
    };
    if tokens.get(start).is_some_and(|token| token.is_word("WITH"))
        && cte_contains_dml(tokens, start)
    {
        return StatementClass::Dml;
    }
    classify_command(tokens, command, explain_allowed)
}

pub(super) fn command_index(tokens: &[Token<'_>], mut index: usize) -> Option<usize> {
    if tokens.get(index)?.is_word("WITH") {
        index += 1;
        if tokens
            .get(index)
            .is_some_and(|token| token.is_word("RECURSIVE"))
        {
            index += 1;
        }
        loop {
            if !tokens.get(index).is_some_and(is_identifier) {
                return None;
            }
            index += 1;
            if tokens.get(index).is_some_and(|token| token.is_symbol("(")) {
                index = after_group(tokens, index)?;
            }
            if !tokens.get(index).is_some_and(|token| token.is_word("AS")) {
                return None;
            }
            index += 1;
            if tokens.get(index).is_some_and(|token| token.is_word("NOT")) {
                index += 1;
                if !tokens
                    .get(index)
                    .is_some_and(|token| token.is_word("MATERIALIZED"))
                {
                    return None;
                }
                index += 1;
            } else if tokens
                .get(index)
                .is_some_and(|token| token.is_word("MATERIALIZED"))
            {
                index += 1;
            }
            if !tokens.get(index).is_some_and(|token| token.is_symbol("(")) {
                return None;
            }
            index = after_group(tokens, index)?;
            if tokens.get(index).is_some_and(|token| token.is_symbol(",")) {
                index += 1;
            } else {
                break;
            }
        }
    }
    (index < tokens.len()).then_some(index)
}

fn cte_contains_dml(tokens: &[Token<'_>], start: usize) -> bool {
    let mut index = start + 1;
    if tokens
        .get(index)
        .is_some_and(|token| token.is_word("RECURSIVE"))
    {
        index += 1;
    }
    loop {
        if !tokens.get(index).is_some_and(is_identifier) {
            return false;
        }
        index += 1;
        if tokens.get(index).is_some_and(|token| token.is_symbol("(")) {
            let Some(after_columns) = after_group(tokens, index) else {
                return false;
            };
            index = after_columns;
        }
        if !tokens.get(index).is_some_and(|token| token.is_word("AS")) {
            return false;
        }
        index += 1;
        if tokens.get(index).is_some_and(|token| token.is_word("NOT")) {
            index += 1;
            if !tokens
                .get(index)
                .is_some_and(|token| token.is_word("MATERIALIZED"))
            {
                return false;
            }
            index += 1;
        } else if tokens
            .get(index)
            .is_some_and(|token| token.is_word("MATERIALIZED"))
        {
            index += 1;
        }
        if !tokens.get(index).is_some_and(|token| token.is_symbol("(")) {
            return false;
        }
        let body_start = index + 1;
        let Some(after_body) = after_group(tokens, index) else {
            return false;
        };
        if command_index(tokens, body_start).is_some_and(|body_command| {
            tokens.get(body_command).is_some_and(|token| {
                ["INSERT", "UPDATE", "DELETE", "MERGE", "COPY"]
                    .iter()
                    .any(|word| token.is_word(word))
            })
        }) {
            return true;
        }
        index = after_body;
        if tokens.get(index).is_some_and(|token| token.is_symbol(",")) {
            index += 1;
        } else {
            return false;
        }
    }
}

pub(super) fn is_identifier(token: &Token<'_>) -> bool {
    matches!(token.kind, TokenKind::Word | TokenKind::QuotedIdentifier)
}

pub(super) fn after_group(tokens: &[Token<'_>], open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if token.is_symbol("(") {
            depth += 1;
        } else if token.is_symbol(")") {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index + 1);
            }
        }
    }
    None
}

pub(super) fn classify_command(
    tokens: &[Token<'_>],
    command: usize,
    explain_allowed: bool,
) -> StatementClass {
    let Some(token) = tokens.get(command) else {
        return StatementClass::Other;
    };
    if token.is_word("EXPLAIN") && explain_allowed {
        // EXPLAIN without ANALYZE is always read-only; ANALYZE inherits the target class.
        let analyze = explain_has_analyze(tokens, command);
        if !analyze {
            return StatementClass::Query;
        }
        let target = explain_target_start(tokens, command);
        return target.map_or(StatementClass::Other, |target| {
            classify_statement(tokens, target, false)
        });
    }
    if ["SELECT", "VALUES", "TABLE", "SHOW"]
        .iter()
        .any(|word| token.is_word(word))
    {
        StatementClass::Query
    } else if ["INSERT", "UPDATE", "DELETE", "MERGE", "COPY"]
        .iter()
        .any(|word| token.is_word(word))
    {
        StatementClass::Dml
    } else if [
        "CREATE", "ALTER", "DROP", "TRUNCATE", "COMMENT", "GRANT", "REVOKE", "RENAME",
    ]
    .iter()
    .any(|word| token.is_word(word))
    {
        StatementClass::Ddl
    } else if [
        "BEGIN",
        "START",
        "COMMIT",
        "END",
        "ROLLBACK",
        "ABORT",
        "SAVEPOINT",
        "RELEASE",
    ]
    .iter()
    .any(|word| token.is_word(word))
    {
        StatementClass::Transaction
    } else if ["SET", "RESET", "DISCARD"]
        .iter()
        .any(|word| token.is_word(word))
    {
        StatementClass::Session
    } else {
        StatementClass::Other
    }
}

fn explain_has_analyze(tokens: &[Token<'_>], explain: usize) -> bool {
    let mut index = explain + 1;
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
        index = end;
    }
    tokens
        .get(index)
        .is_some_and(|token| token.is_word("ANALYZE"))
}

fn explain_target_start(tokens: &[Token<'_>], explain: usize) -> Option<usize> {
    let mut index = explain + 1;
    if tokens.get(index).is_some_and(|token| token.is_symbol("(")) {
        index = after_group(tokens, index)?;
    }
    while tokens.get(index).is_some_and(|token| {
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
        index += 1;
        if tokens.get(index).is_some_and(|token| {
            token.is_word("TRUE")
                || token.is_word("FALSE")
                || token.is_word("JSON")
                || token.is_word("TEXT")
                || token.is_word("XML")
                || token.is_word("YAML")
        }) {
            index += 1;
        }
    }
    (index < tokens.len()).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::{StatementClass, classify, split_statements};

    #[test]
    fn splits_only_on_semicolons_outside_quotes_and_comments() {
        let sql = "SELECT ';', E'it\\'s; fine', \"semi;colon\"; /* a; /* nested; */ done */ SELECT 2; -- ;\nSELECT 3";
        let spans = split_statements(sql);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].text, "SELECT ';', E'it\\'s; fine', \"semi;colon\"");
        assert!(spans[1].text.contains("nested;"));
        assert_eq!(spans[2].text, "-- ;\nSELECT 3");
        assert_eq!(
            spans.iter().map(|span| span.index).collect::<Vec<_>>(),
            [0, 1, 2]
        );
    }

    #[test]
    fn splitter_handles_lexical_edge_cases_table_driven() {
        let cases: &[(&str, &[&str])] = &[
            ("SELECT ';'; SELECT 2", &["SELECT ';'", "SELECT 2"]),
            (
                r"SELECT E'it\'s; fine'; SELECT 2",
                &[r"SELECT E'it\'s; fine'", "SELECT 2"],
            ),
            (r"SELECT E'\''; SELECT 2", &[r"SELECT E'\''", "SELECT 2"]),
            (
                "DO $tag$ BEGIN PERFORM ';'; END $tag$; SELECT 2",
                &["DO $tag$ BEGIN PERFORM ';'; END $tag$", "SELECT 2"],
            ),
            (
                "SELECT foo$tag$bar; SELECT 2",
                &["SELECT foo$tag$bar", "SELECT 2"],
            ),
            (
                "/* outer; /* nested; */ end */ SELECT 1; SELECT 2",
                &["/* outer; /* nested; */ end */ SELECT 1", "SELECT 2"],
            ),
        ];
        for (sql, expected) in cases {
            let actual = split_statements(sql)
                .into_iter()
                .map(|span| span.text)
                .collect::<Vec<_>>();
            assert_eq!(actual.as_slice(), *expected, "{sql}");
        }
    }

    #[test]
    fn splits_dollar_quoted_bodies_with_tags() {
        let spans = split_statements("DO $tag$ BEGIN PERFORM ';'; END $tag$; SELECT $$x;y$$;");
        assert_eq!(spans.len(), 2);
        assert!(spans[0].text.contains("PERFORM ';'"));
        assert_eq!(spans[1].text, "SELECT $$x;y$$");
    }

    #[test]
    fn ignores_comment_only_and_empty_statements_and_tracks_byte_offsets() {
        let sql = "; -- empty;\n /* only /* nested */ comment */ ;\nSELECT 'é';";
        let spans = split_statements(sql);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].index, 0);
        assert_eq!(&sql[spans[0].start_offset..], "SELECT 'é';");
    }

    #[test]
    fn classifies_with_and_explain_statements() {
        let cases = [
            (
                "WITH c AS (SELECT 1) SELECT * FROM c",
                StatementClass::Query,
            ),
            (
                "WITH c AS (SELECT 1) UPDATE t SET x = 2",
                StatementClass::Dml,
            ),
            ("WITH c AS (SELECT 1) DELETE FROM t", StatementClass::Dml),
            (
                "WITH removed AS (DELETE FROM t RETURNING id) SELECT id FROM removed",
                StatementClass::Dml,
            ),
            ("VALUES (1)", StatementClass::Query),
            ("EXPLAIN SELECT 1", StatementClass::Query),
            ("EXPLAIN (ANALYZE true) DELETE FROM t", StatementClass::Dml),
            (
                "EXPLAIN (ANALYZE true) WITH gone AS (DELETE FROM t RETURNING id) SELECT * FROM gone",
                StatementClass::Dml,
            ),
            (
                "EXPLAIN WITH gone AS (DELETE FROM t RETURNING id) SELECT * FROM gone",
                StatementClass::Query,
            ),
            (
                "EXPLAIN (ANALYZE false) UPDATE t SET x = 2",
                StatementClass::Query,
            ),
            ("SET search_path = public", StatementClass::Session),
            ("CREATE TABLE t (id int)", StatementClass::Ddl),
        ];
        for (sql, expected) in cases {
            assert_eq!(classify(sql), expected, "{sql}");
        }
    }

    #[test]
    fn malformed_sql_is_classified_without_panicking() {
        assert_eq!(classify("SELECT 'unterminated"), StatementClass::Other);
        assert_eq!(split_statements("SELECT 'unterminated; SELECT 2").len(), 1);
    }
}
