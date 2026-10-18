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
    /// The cover's file id and hash.
    cover: Option<(String, Option<Vec<u8>>)>,
}

impl ReleaseDeletion {
    pub fn release_id(&self) -> &str {
        &self.release_id
    }

    pub fn album_id(&self) -> &str {
        &self.album_id
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
        let cover = sql
            .query_row(
                "SELECT blob_id, hash FROM covers WHERE id = ?",
                params![release_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<Vec<u8>>>(1)?)),
            )
            .optional()?;
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
