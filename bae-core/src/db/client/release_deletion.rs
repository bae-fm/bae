use super::*;

/// The rows a deletion was planned against. Rechecking inside the write refuses
/// a deletion when the release moved or its attached files changed meanwhile.
/// Coven observes the deleted file rows and manages their retained bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseDeletion {
    release_id: String,
    album_id: String,
    /// File ids and hashes, ordered by id.
    files: Vec<(String, Option<Vec<u8>>)>,
    /// The cover's version, when the release has a cover.
    cover: Option<String>,
}

impl ReleaseDeletion {
    pub fn release_id(&self) -> &str {
        &self.release_id
    }

    pub fn album_id(&self) -> &str {
        &self.album_id
    }

    /// The rows carrying the release's files: each release file, and its
    /// cover, keyed by the release.
    pub(super) fn file_rows(&self) -> Vec<(&'static str, String)> {
        self.files
            .iter()
            .map(|(id, _)| ("release_files", id.clone()))
            .chain([("covers", self.release_id.clone())])
            .collect()
    }

    pub(super) fn plan_on<Q: QueryOne + QueryRows>(
        sql: &Q,
        release_id: &str,
    ) -> Result<Self, DbError> {
        let album_id = sql
            .query_row(
                "SELECT album_id FROM releases WHERE id = ?",
                params![release_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| DbError::Message(format!("release not found: {release_id}")))?;
        let files = sql.query(
            "SELECT id, hash FROM release_files WHERE release_id = ? ORDER BY id",
            params![release_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<Vec<u8>>>(1)?)),
        )?;
        let cover = super::files::image_version_on(sql, &LibraryImageType::Cover, release_id)?;
        Ok(Self {
            release_id: release_id.to_string(),
            album_id,
            files,
            cover,
        })
    }

    pub(super) fn apply_on(&self, sql: &SqlContext<'_, '_>) -> Result<(), DbError> {
        let current = Self::plan_on(sql, &self.release_id)?;
        if current != *self {
            return Err(DbError::Message(format!(
                "delete plan for release {} changed after planning",
                self.release_id
            )));
        }
        Ok(())
    }
}
