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

/// Why a deletion was refused. Nothing of it landed; the person may ask for
/// it again.
#[derive(Debug, thiserror::Error)]
pub enum DeletionRefusal {
    /// What the deletion deletes changed between its plan and its write: a
    /// release moved, came or went, or its files or cover changed.
    #[error("the delete plan for {0} changed after planning")]
    PlanChanged(Deleted),
}

/// Why a deletion did not commit.
pub type DeletionError = WriteError<DeletionRefusal>;

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
            return Err(WriteError::Refused(DeletionRefusal::PlanChanged(Deleted::Release(
                self.release_id.clone(),
            ))));
        }
        Ok(())
    }
}
