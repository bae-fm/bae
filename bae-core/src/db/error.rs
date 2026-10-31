/// Failures of bae's database operations, preserving the originating cause.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    FileRead(#[from] coven::FileReadError),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Coven(coven::CovenError),
    #[error(transparent)]
    Creation(#[from] coven::StoreCreationError),
    #[error("{0}")]
    Message(String),
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
            error => Self::Coven(error),
        }
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
            DbError::Coven(error) => error,
            error => rusqlite::Error::UserFunctionError(Box::new(error)).into(),
        }
    }
}
