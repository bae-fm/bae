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
    /// File ids and the coven files they reference, in the release's file
    /// order.
    files: Vec<(String, coven::FileId)>,
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

    pub(super) fn plan_on<Q: bae_store::WriterRead>(
        sql: &Q,
        release_id: &str,
    ) -> Result<Option<Self>, DbError> {
        let Some(release) = super::release_with_files::release_with_files_on(sql, release_id)?
        else {
            return Ok(None);
        };
        Ok(Some(Self {
            release_id: release_id.to_string(),
            album_id: release.album_id,
            files: release.files.into_iter().map(|file| (file.id, file.file)).collect(),
            cover: release.cover_file,
        }))
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
