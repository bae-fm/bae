/// Failures of bae's database operations, preserving the originating cause.
///
/// Coven's errors are boxed: each is near the size clippy allows a Result's
/// error, and every error of bae's holding this one would pass it.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    FileRead(Box<coven::FileReadError>),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Coven(Box<coven::CovenError>),
    #[error(transparent)]
    Creation(Box<coven::StoreCreationError>),
    /// A stored value no writer here produces: the row is damaged, or one
    /// nothing here wrote.
    #[error("{what} holds {stored}, which nothing here writes")]
    Unreadable { what: String, stored: String },
    /// A stored JSON column that does not decode.
    #[error("{what} holds {json}, which nothing here writes")]
    UnreadableJson {
        what: String,
        json: String,
        #[source]
        source: serde_json::Error,
    },
    /// A value to write or bind that does not encode as JSON.
    #[error("{what} does not encode as JSON")]
    Encode {
        what: String,
        #[source]
        source: serde_json::Error,
    },
    /// A value to write that the integer SQLite stores cannot hold.
    #[error("{what} {value} exceeds SQLite's integer range")]
    OutOfRange { what: String, value: String },
    /// A stored path whose bytes this platform never records.
    #[error("a stored path does not open on this device")]
    StoredPath(#[source] coven::PathBytesError),
    /// A write failed and rolling it back failed too.
    #[error("{operation}; rollback failed: {rollback}")]
    Rollback {
        operation: Box<DbError>,
        #[source]
        rollback: rusqlite::Error,
    },
    /// A write failed, or committed, and removing released file bytes failed
    /// too, each failure kept.
    #[error("file cleanup failed: {failures:?}; write: {write}")]
    FileCleanup { write: Box<DbError>, failures: Vec<coven::DbError> },
}

impl DbError {
    /// `stored`, read from `what` (a table's column, a stored list), is a
    /// value no writer here produces.
    pub fn unreadable(what: impl Into<String>, stored: impl std::fmt::Debug) -> Self {
        Self::Unreadable { what: what.into(), stored: format!("{stored:?}") }
    }

    /// The JSON `json`, read from `what`, does not decode.
    pub fn unreadable_json(
        what: impl Into<String>,
        json: impl Into<String>,
        source: serde_json::Error,
    ) -> Self {
        Self::UnreadableJson { what: what.into(), json: json.into(), source }
    }

    /// The value written or bound as `what` does not encode as JSON.
    pub fn encode(what: impl Into<String>, source: serde_json::Error) -> Self {
        Self::Encode { what: what.into(), source }
    }
}

impl From<coven::CovenError> for DbError {
    fn from(error: coven::CovenError) -> Self {
        match error {
            coven::CovenError::Database(coven::DbError::Sqlite(error)) => Self::Sqlite(error),
            error => Self::Coven(Box::new(error)),
        }
    }
}

impl From<coven::FileReadError> for DbError {
    fn from(error: coven::FileReadError) -> Self {
        Self::FileRead(Box::new(error))
    }
}

impl From<coven::StoreCreationError> for DbError {
    fn from(error: coven::StoreCreationError) -> Self {
        Self::Creation(Box::new(error))
    }
}

impl From<coven::SyncError> for DbError {
    fn from(error: coven::SyncError) -> Self {
        coven::CovenError::from(error).into()
    }
}

impl From<coven::DbError> for DbError {
    fn from(error: coven::DbError) -> Self {
        coven::CovenError::from(error).into()
    }
}

/// A bug a coven call carried back, which the store reported as the call
/// returned, or one coven repeats; any other failure holds none.
impl bae_util::task_failure::ReportedBug for DbError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Io(error) => error.reported_bug(),
            Self::FileRead(error) => error.reported_bug(),
            Self::Coven(error) => error.reported_bug(),
            Self::Creation(error) => error.reported_bug(),
            Self::Rollback { operation, .. } => operation.reported_bug(),
            Self::FileCleanup { write, failures } => write.reported_bug().or_else(|| {
                failures.iter().find_map(bae_util::task_failure::ReportedBug::reported_bug)
            }),
            Self::Sqlite(_)
            | Self::Unreadable { .. }
            | Self::UnreadableJson { .. }
            | Self::Encode { .. }
            | Self::OutOfRange { .. }
            | Self::StoredPath(_) => None,
        }
    }
}

/// A database failure that carries a bug of coven's work, itself or through
/// the failure it keeps.
impl coven::task::CarriesBug for DbError {
    fn bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Io(error) => error.bug(),
            Self::FileRead(error) => error.bug(),
            Self::Coven(error) => error.bug(),
            Self::Creation(error) => error.bug(),
            Self::Rollback { operation, .. } => operation.bug(),
            Self::FileCleanup { write, failures } => {
                write.bug().or_else(|| failures.iter().find_map(coven::task::CarriesBug::bug))
            }
            Self::Sqlite(_)
            | Self::Unreadable { .. }
            | Self::UnreadableJson { .. }
            | Self::Encode { .. }
            | Self::OutOfRange { .. }
            | Self::StoredPath(_) => None,
        }
    }
}

/// A write failing with `DbError` keeps a failed rollback or byte removal in
/// the error it wraps: coven's in its own, any other beside it.
impl coven::WriteFailure for DbError {
    fn with_rollback(self, rollback: rusqlite::Error) -> Self {
        match self {
            Self::Coven(error) => Self::Coven(Box::new(error.with_rollback(rollback))),
            error => Self::Rollback { operation: Box::new(error), rollback },
        }
    }

    fn with_cleanup(write: Self, failures: Vec<coven::DbError>) -> Self {
        match write {
            Self::Coven(error) => {
                Self::Coven(Box::new(coven::CovenError::with_cleanup(*error, failures)))
            }
            error => Self::FileCleanup { write: Box::new(error), failures },
        }
    }
}
