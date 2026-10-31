use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DbErrorKind {
    /// Network/TLS/handshake failure.
    Connect,
    /// Rejected credentials or expired IAM token.
    Auth,
    /// Server returned an error for a statement.
    Query,
    Canceled,
    Timeout,
    /// The engine cannot do what was asked (see `Capabilities`).
    Unsupported,
    /// No open connection for the `ConnectionKey`.
    NotConnected,
    Internal,
}

/// Database failure shown to the user next to the statement that caused it.
#[derive(Debug, Clone, Serialize)]
pub struct DbError {
    pub kind: DbErrorKind,
    pub message: String,
    /// SQLSTATE.
    pub code: Option<String>,
    pub detail: Option<String>,
    pub hint: Option<String>,
    /// 1-based character offset into the statement.
    pub position: Option<u32>,
}

impl DbError {
    #[must_use]
    pub fn new(kind: DbErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            code: None,
            detail: None,
            hint: None,
            position: None,
        }
    }
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.code {
            Some(code) => write!(f, "{} ({code})", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for DbError {}

impl From<DbError> for gluon::AppError {
    fn from(e: DbError) -> Self {
        match e.kind {
            DbErrorKind::Internal => gluon::AppError::Internal(Box::new(e)),
            DbErrorKind::NotConnected => gluon::AppError::Conflict(e.to_string()),
            _ => gluon::AppError::BadRequest(e.to_string()),
        }
    }
}
