use super::*;

/// What a deletion deletes: one release, or every release of an album.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deleted {
    Release(String),
    Album(String),
}

impl std::fmt::Display for Deleted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Release(id) => write!(f, "release {id}"),
            Self::Album(id) => write!(f, "album {id}"),
        }
    }
}

/// Why a deletion did not commit. Nothing of it landed; the person may ask
/// for it again.
#[derive(Debug, thiserror::Error)]
pub enum DeletionError {
    /// What the deletion deletes changed between its plan and its write: a
    /// release moved, came or went, or its files or cover changed.
    #[error("the delete plan for {0} changed after planning")]
    PlanChanged(Deleted),
    #[error(transparent)]
    Database(#[from] DbError),
    /// The deletion was refused, and rolling it back failed too.
    #[error("{operation}; rollback failed: {rollback}")]
    Rollback {
        operation: Box<DeletionError>,
        #[source]
        rollback: rusqlite::Error,
    },
    /// The deletion was refused, and removing the bytes it released failed
    /// too, each failure kept.
    #[error("file cleanup failed: {failures:?}; write: {write}")]
    FileCleanup { write: Box<DeletionError>, failures: Vec<coven::DbError> },
}

impl From<rusqlite::Error> for DeletionError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.into())
    }
}

impl From<coven::DbError> for DeletionError {
    fn from(error: coven::DbError) -> Self {
        Self::Database(error.into())
    }
}

impl coven::task::CarriesBug for DeletionError {
    fn bug(&self) -> Option<&coven::task::Bug> {
        use coven::task::CarriesBug;
        match self {
            Self::Database(error) => error.bug(),
            Self::Rollback { operation, .. } => operation.bug(),
            Self::FileCleanup { write, failures } => {
                write.bug().or_else(|| failures.iter().find_map(CarriesBug::bug))
            }
            Self::PlanChanged(_) => None,
        }
    }
}

/// A bug a coven call carried back, which the store reported as it returned.
impl bae_util::task_failure::ReportedBug for DeletionError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Database(error) => error.reported_bug(),
            Self::Rollback { operation, .. } => operation.reported_bug(),
            Self::FileCleanup { write, failures } => write.reported_bug().or_else(|| {
                failures.iter().find_map(bae_util::task_failure::ReportedBug::reported_bug)
            }),
            Self::PlanChanged(_) => None,
        }
    }
}

/// A failed rollback or byte removal is kept beside the failure it followed:
/// a store failure keeps it in its own, a refusal stays the refusal it is.
impl coven::WriteFailure for DeletionError {
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

/// The rows a deletion was planned against. Rechecking inside the write refuses
/// a deletion when the release moved or its files changed meanwhile. Each coven
/// file the deleted rows referenced goes with its last reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseDeletion {
    release_id: String,
    album_id: String,
    /// File ids and the coven files they reference, ordered by id.
    files: Vec<(String, Option<coven::FileId>)>,
    /// The cover's coven file, when the release has a cover.
    cover: Option<coven::FileId>,
}

impl ReleaseDeletion {
    pub fn release_id(&self) -> &str {
        &self.release_id
    }

    pub fn album_id(&self) -> &str {
        &self.album_id
    }

    pub(super) fn plan_on<Q: coven::SqlRead>(
        sql: &Q,
        release_id: &str,
    ) -> Result<Option<Self>, DbError> {
        let Some(album_id) = sql
            .query_row("SELECT album_id FROM releases WHERE id = ?", params![release_id], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
        else {
            return Ok(None);
        };
        let files = sql.query(
            "SELECT id, file FROM release_files WHERE release_id = ? ORDER BY id",
            params![release_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<coven::FileId>>(1)?)),
        )?;
        let cover = super::files::image_file_on(sql, &LibraryImageType::Cover, release_id)?;
        Ok(Some(Self { release_id: release_id.to_string(), album_id, files, cover }))
    }

    pub(super) fn apply_on(&self, sql: &CovenWrite<'_>) -> Result<(), DeletionError> {
        let current = Self::plan_on(sql, &self.release_id)?;
        if current.as_ref() != Some(self) {
            return Err(DeletionError::PlanChanged(Deleted::Release(self.release_id.clone())));
        }
        Ok(())
    }
}
