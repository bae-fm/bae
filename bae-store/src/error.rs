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

/// What a store write ended in: the domain's refusal, the store's failure, or
/// either with a failed rollback or file cleanup kept beside it. A refusal
/// carries no bug of its own.
#[derive(Debug)]
pub enum WriteError<R> {
    Refused(R),
    Database(DbError),
    /// The write was refused, and rolling it back failed too.
    Rollback {
        operation: Box<WriteError<R>>,
        rollback: rusqlite::Error,
    },
    /// The write was refused, and removing the bytes it staged or released
    /// failed too, each failure kept.
    FileCleanup {
        write: Box<WriteError<R>>,
        failures: Vec<coven::DbError>,
    },
}

// Written out: the derive's bound on the boxed `Self` overflows.
impl<R: std::fmt::Display> std::fmt::Display for WriteError<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(refusal) => refusal.fmt(f),
            Self::Database(error) => error.fmt(f),
            Self::Rollback { operation, rollback } => {
                write!(f, "{operation}; rollback failed: {rollback}")
            }
            Self::FileCleanup { write, failures } => {
                write!(f, "file cleanup failed: {failures:?}; write: {write}")
            }
        }
    }
}

impl<R: std::error::Error + 'static> std::error::Error for WriteError<R> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Refused(refusal) => refusal.source(),
            Self::Database(error) => std::error::Error::source(error),
            Self::Rollback { rollback, .. } => Some(rollback),
            Self::FileCleanup { .. } => None,
        }
    }
}

impl<R> WriteError<R> {
    /// The same write ending, its refusal (wherever a kept rollback or file
    /// cleanup sits around it) read as `f` reads it.
    pub fn map_refusal<S>(self, f: impl FnOnce(R) -> S) -> WriteError<S> {
        match self {
            Self::Refused(refusal) => WriteError::Refused(f(refusal)),
            Self::Database(error) => WriteError::Database(error),
            Self::Rollback { operation, rollback } => {
                WriteError::Rollback { operation: Box::new(operation.map_refusal(f)), rollback }
            }
            Self::FileCleanup { write, failures } => {
                WriteError::FileCleanup { write: Box::new(write.map_refusal(f)), failures }
            }
        }
    }
}

impl<R> From<DbError> for WriteError<R> {
    fn from(error: DbError) -> Self {
        Self::Database(error)
    }
}

impl<R> From<rusqlite::Error> for WriteError<R> {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.into())
    }
}

impl<R> From<coven::DbError> for WriteError<R> {
    fn from(error: coven::DbError) -> Self {
        Self::Database(error.into())
    }
}

impl<R> coven::task::CarriesBug for WriteError<R> {
    fn bug(&self) -> Option<&coven::task::Bug> {
        use coven::task::CarriesBug;
        match self {
            Self::Database(error) => error.bug(),
            Self::Rollback { operation, .. } => operation.bug(),
            Self::FileCleanup { write, failures } => {
                write.bug().or_else(|| failures.iter().find_map(CarriesBug::bug))
            }
            Self::Refused(_) => None,
        }
    }
}

/// A bug a coven call carried back, which the store reported as it returned.
impl<R> bae_util::task_failure::ReportedBug for WriteError<R> {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        use bae_util::task_failure::ReportedBug;
        match self {
            Self::Database(error) => error.reported_bug(),
            Self::Rollback { operation, .. } => operation.reported_bug(),
            Self::FileCleanup { write, failures } => {
                write.reported_bug().or_else(|| failures.iter().find_map(ReportedBug::reported_bug))
            }
            Self::Refused(_) => None,
        }
    }
}

/// A failed rollback or byte removal is kept beside the failure it followed:
/// a store failure keeps it in its own, a refusal stays the refusal it is.
impl<R> coven::WriteFailure for WriteError<R> {
    fn with_rollback(self, rollback: rusqlite::Error) -> Self {
        match self {
            Self::Database(error) => {
                Self::Database(coven::WriteFailure::with_rollback(error, rollback))
            }
            operation => Self::Rollback { operation: Box::new(operation), rollback },
        }
    }

    fn with_cleanup(write: Self, failures: Vec<coven::DbError>) -> Self {
        match write {
            Self::Database(error) => {
                Self::Database(coven::WriteFailure::with_cleanup(error, failures))
            }
            write => Self::FileCleanup { write: Box::new(write), failures },
        }
    }
}
