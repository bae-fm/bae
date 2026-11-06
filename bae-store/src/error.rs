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
    /// A failure of a type a crate above this one defines, such as a stored
    /// name that names no file, kept as its own value.
    #[error(transparent)]
    Cause(Box<dyn std::error::Error + Send + Sync>),
    #[error("{0}")]
    Message(String),
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

    /// `cause`, a failure of a type above this crate, as a database error.
    pub fn cause(cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Cause(Box::new(cause))
    }
}

impl From<coven::CovenError> for DbError {
    fn from(error: coven::CovenError) -> Self {
        match error {
            coven::CovenError::Database(coven::DbError::Sqlite(
                rusqlite::Error::UserFunctionError(error),
            )) => match error.downcast::<Self>() {
                Ok(error) => *error,
                Err(error) => Self::Sqlite(rusqlite::Error::UserFunctionError(error)),
            },
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

impl From<coven::DbError> for DbError {
    fn from(error: coven::DbError) -> Self {
        coven::CovenError::from(error).into()
    }
}

impl From<DbError> for coven::CovenError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::Sqlite(error) => error.into(),
            DbError::Coven(error) => *error,
            error => rusqlite::Error::UserFunctionError(Box::new(error)).into(),
        }
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
            Self::Sqlite(_)
            | Self::Unreadable { .. }
            | Self::UnreadableJson { .. }
            | Self::Encode { .. }
            | Self::OutOfRange { .. }
            | Self::StoredPath(_)
            | Self::Cause(_)
            | Self::Message(_) => None,
        }
    }
}
