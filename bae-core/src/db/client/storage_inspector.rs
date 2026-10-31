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
/// `coven_downloading` and `coven_uploads`, each joined by the coven file the
/// row references, and the path of the original this device holds of it. A
/// pin's download lists its file in `coven_downloading` before `coven_pins`
/// holds a row for it.
pub(super) const INSPECTOR_FILES: &str = concat!(
    "SELECT ",
    release_file_columns!(),
    ", pin.file IS NOT NULL AS pin_kept, \
        downloading.file AS downloading_file, \
        upload.file AS upload_file, upload.sending AS upload_sending, \
        upload.pending AS upload_pending, original.path AS original_path \
     FROM release_files AS f \
     LEFT JOIN coven_pins AS pin ON pin.file = f.file \
     LEFT JOIN coven_downloading AS downloading ON downloading.file = f.file \
     LEFT JOIN coven_uploads AS upload ON upload.file = f.file \
     LEFT JOIN local_release_files AS original ON original.file_id = f.id \
     WHERE f.release_id = ?1 \
     ORDER BY f.original_filename COLLATE natural_order, f.id"
);

/// The records of moves whose upload found an original missing or changed:
/// what an upload whose own record says nothing waits for. coven keeps them
/// on the move, naming the original by its path.
const MOVE_RECORDS: &str = "SELECT record FROM coven_pending WHERE file IS NULL";

pub(super) fn inspector_files_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbStorageInspectorFile>, DbError> {
    let moves: Vec<coven::PendingRecord> = sql
        .query(MOVE_RECORDS, [], |row| {
            row.get::<_, coven::PendingRecord>(0)
        })?
        .into_iter()
        .filter(|record| matches!(record.subject, coven::PendingSubject::Move { .. }))
        .collect();
    // The record of the move whose upload found the original whose path's
    // bytes are `path`.
    let move_record = |path: &[u8]| {
        moves
            .iter()
            .find(|record| {
                matches!(&record.reason, coven::PendingReason::Original { path: original, .. }
                    if coven::path_bytes(original) == path)
            })
            .cloned()
    };
    Ok(sql.query(INSPECTOR_FILES, params![release_id], |row| {
        let pin = match (
            row.get::<_, bool>("pin_kept")?,
            row.get::<_, Option<coven::FileId>>("downloading_file")?,
        ) {
            (true, _) => Some(crate::album_detail::FilePin::Kept),
            (false, Some(file)) => Some(crate::album_detail::FilePin::Downloading { file }),
            (false, None) => None,
        };
        let upload = match row.get::<_, Option<coven::FileId>>("upload_file")? {
            Some(file) => Some(DbFileUpload {
                file,
                sending: row.get("upload_sending")?,
                pending: match row.get::<_, Option<coven::PendingRecord>>("upload_pending")? {
                    Some(record) => Some(record),
                    None => row
                        .get::<_, Option<Vec<u8>>>("original_path")?
                        .and_then(|path| move_record(&path)),
                },
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
