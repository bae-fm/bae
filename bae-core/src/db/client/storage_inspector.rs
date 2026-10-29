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
        files: inspector_files_on(sql, release_id)?,
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

/// A release's files as the inspector shows them, in the order of
/// `RELEASE_FILES_IN_ORDER`, each with where this device's pin of it stands
/// and its upload while it waits in this device's queue, from coven's `coven_pins`,
/// `coven_downloading` and `coven_uploads`, each joined by the file's coven
/// key. A pin's download lists its file in `coven_downloading` before
/// `coven_pins` holds a row for it.
pub(super) const INSPECTOR_FILES: &str = "SELECT f.*, \
        pin.key IS NOT NULL AS pin_held, pin.kept IS 1 AS pin_kept, \
        downloading.file AS downloading_file, \
        upload.file AS upload_file, upload.sending AS upload_sending, \
        upload.pending AS upload_pending \
     FROM release_files AS f \
     LEFT JOIN coven_pins AS pin \
         ON pin.table_name = 'release_files' AND pin.key = coven_key(f.id) \
     LEFT JOIN coven_downloading AS downloading \
         ON downloading.table_name = 'release_files' AND downloading.key = coven_key(f.id) \
     LEFT JOIN coven_uploads AS upload \
         ON upload.table_name = 'release_files' AND upload.key = coven_key(f.id) \
     WHERE f.release_id = ?1 \
     ORDER BY f.original_filename COLLATE natural_order, f.id";

pub(super) fn inspector_files_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbStorageInspectorFile>, DbError> {
    Ok(sql.query(INSPECTOR_FILES, params![release_id], |row| {
        let pin = match (
            row.get::<_, bool>("pin_kept")?,
            row.get::<_, Option<coven::FileId>>("downloading_file")?,
            row.get::<_, bool>("pin_held")?,
        ) {
            (true, _, _) => Some(crate::album_detail::FilePin::Kept),
            (false, Some(file), _) => Some(crate::album_detail::FilePin::Downloading { file }),
            (false, None, true) => Some(crate::album_detail::FilePin::Waiting),
            (false, None, false) => None,
        };
        let upload = match row.get::<_, Option<coven::FileId>>("upload_file")? {
            Some(file) => Some(DbFileUpload {
                file,
                sending: row.get("upload_sending")?,
                pending: row.get("upload_pending")?,
            }),
            None => None,
        };
        Ok(DbStorageInspectorFile {
            file: row_to_file(row)?,
            pin,
            upload,
        })
    })?)
}
