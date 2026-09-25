use super::*;

/// What deleting one release removes that coven has to be told about in the
/// same write.
///
/// coven reclaims the on-device copy of a blob — a cover in its own store, a
/// file's cached or pinned copy — only when the write that stops referencing it
/// declares it deleted; a bare row DELETE leaves the bytes behind for good. A
/// Local release's files are the user's own files in place, registered with
/// coven as external files, and the delete drops those registrations. Both
/// have to be known before coven opens the write, so this is read first and
/// `apply_on` checks it against the rows inside the write,
/// refusing a plan the library has moved past rather than deleting rows whose
/// blobs it did not declare.
///
/// The release's cloud objects are not represented here: a row that stops
/// naming a blob leaves an orphan that coven's accepted reclaim retires, and an
/// in-flight make-Remote is unwound by coven when its root goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseDeletion {
    release_id: String,
    album_id: String,
    remote: bool,
    /// Each `release_files` row's id (its blob id) and `cloud_path`, by id.
    files: Vec<(String, Option<String>)>,
    /// The `covers` row's `blob_id` and `cloud_path`.
    cover: Option<(String, Option<String>)>,
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
        let (album_id, remote) = sql
            .query_row(
                "SELECT album_id, remote FROM releases WHERE id = ?",
                params![release_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
            )
            .optional()?
            .ok_or_else(|| DbError::Message(format!("release not found: {release_id}")))?;
        let files = sql.query(
            "SELECT id, cloud_path FROM release_files WHERE release_id = ? ORDER BY id",
            params![release_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )?;
        let cover = sql
            .query_row(
                "SELECT blob_id, cloud_path FROM covers WHERE id = ?",
                params![release_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        Ok(Self {
            release_id: release_id.to_string(),
            album_id,
            remote,
            files,
            cover,
        })
    }

    /// Every blob the release's rows carry, for the write to declare deleted.
    pub(super) fn blob_deletes(&self) -> impl Iterator<Item = coven::BlobRef> + '_ {
        let files = self.files.iter().map(|(file_id, cloud_path)| {
            crate::sync::release_file_blob_ref(file_id, cloud_path.clone())
        });
        let cover = self.cover.iter().map(|(blob_id, cloud_path)| {
            crate::sync::image_blob_ref(crate::sync::COVERS_NAMESPACE, blob_id, cloud_path.clone())
        });
        files.chain(cover)
    }

    /// Inside the delete's write, before the release row goes: refuse the plan
    /// if the rows no longer match it, then drop a Local release's external
    /// file registrations while their rows are still there to bind.
    pub(super) fn apply_on(&self, sql: &SqlContext<'_, '_>) -> Result<(), DbError> {
        let current = Self::plan_on(sql, &self.release_id)?;
        if current != *self {
            return Err(DbError::Message(format!(
                "delete plan for release {} changed after planning",
                self.release_id
            )));
        }
        if !self.remote {
            for (file_id, _) in &self.files {
                sql.clear_external_blob(crate::sync::RELEASE_FILES_NAMESPACE, file_id)?;
            }
        }
        Ok(())
    }
}
