//! The Storage Manager inspector's read of one release.

use super::*;

/// The release `release_id` as the inspector shows it, `None` when the
/// library holds no such release: its files with their pins and uploads,
/// what of it waits to upload, and its download and export entries.
fn storage_inspector_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<DbStorageInspector>, DbError> {
    if find_release_by_id_on(sql, release_id)?.is_none() {
        return Ok(None);
    }
    Ok(Some(DbStorageInspector {
        files: release_detail_files_on(sql, release_id)?,
        upload: super::queued_uploads::release_upload_on(sql, release_id)?,
        entries: super::release_queues::release_queue_entries_on(sql, release_id)?,
    }))
}

impl Database {
    /// Follow the release `initial` names, or none, as the Storage Manager's
    /// inspector shows it, each value read in one go. Another release is a
    /// new request on the same query.
    pub(crate) fn subscribe_storage_inspector(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<DbStorageInspector>> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |release_id, sql| {
                let Some(release_id) = release_id else {
                    return Ok(None);
                };
                storage_inspector_on(&sql, release_id).map_err(CovenError::from)
            })
            .process(Ok)
    }
}
