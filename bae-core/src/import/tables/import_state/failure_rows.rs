//! Persisted terminal import failures, including recoverable artist and label
//! identity conflicts.

use super::*;
use crate::import::{IdentityConflict, ImportFailure, ImportFailureReason};

/// The diagnostic column read by both the list and the candidate pane.
pub(crate) const FAILURE_REASON_COLUMNS: &str = "failure.error AS failure_error";

pub(crate) fn failure_reason_from_row(row: &Row<'_>) -> rusqlite::Result<ImportFailureReason> {
    Ok(ImportFailureReason { detail: row.get("failure_error")? })
}

fn existing_artist_from_row(row: &Row<'_>, prefix: &str) -> rusqlite::Result<ExistingArtist> {
    let column = |suffix: &str| format!("{prefix}_{suffix}");
    Ok(ExistingArtist {
        artist_id: row.get(column("id").as_str())?,
        name: row.get(column("name").as_str())?,
        sort_name: row.get(column("sort_name").as_str())?,
        musicbrainz_artist_id: row.get(column("musicbrainz_id").as_str())?,
        discogs_artist_id: row.get(column("discogs_id").as_str())?,
    })
}

fn existing_label_from_row(row: &Row<'_>, prefix: &str) -> rusqlite::Result<ExistingLabel> {
    let column = |suffix: &str| format!("{prefix}_{suffix}");
    Ok(ExistingLabel {
        label_id: row.get(column("id").as_str())?,
        name: row.get(column("name").as_str())?,
        musicbrainz_label_id: row.get(column("musicbrainz_id").as_str())?,
        discogs_label_id: row.get(column("discogs_id").as_str())?,
    })
}

/// The failure the last import of `content_hash` left.
pub(super) fn load_failure_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<Option<ImportFailure>, DbError> {
    sql.query_row(
        &format!(
            "SELECT {FAILURE_REASON_COLUMNS}, failure.failed_at, \
                conflict.incoming_artist_name, \
                conflict.discogs_artist_id AS incoming_discogs_artist_id, \
                conflict.musicbrainz_artist_id AS incoming_musicbrainz_artist_id, \
                discogs.id AS discogs_id, discogs.name AS discogs_name, \
                discogs.sort_name AS discogs_sort_name, \
                discogs.musicbrainz_artist_id AS discogs_musicbrainz_id, \
                discogs.discogs_artist_id AS discogs_discogs_id, \
                musicbrainz.id AS musicbrainz_id, musicbrainz.name AS musicbrainz_name, \
                musicbrainz.sort_name AS musicbrainz_sort_name, \
                musicbrainz.musicbrainz_artist_id AS musicbrainz_musicbrainz_id, \
                musicbrainz.discogs_artist_id AS musicbrainz_discogs_id, \
                label_conflict.incoming_label_name, \
                label_conflict.discogs_label_id AS incoming_discogs_label_id, \
                label_conflict.musicbrainz_label_id AS incoming_musicbrainz_label_id, \
                discogs_label.id AS discogs_label_id, discogs_label.name AS discogs_label_name, \
                discogs_label.musicbrainz_label_id AS discogs_label_musicbrainz_id, \
                discogs_label.discogs_label_id AS discogs_label_discogs_id, \
                musicbrainz_label.id AS musicbrainz_label_id, \
                musicbrainz_label.name AS musicbrainz_label_name, \
                musicbrainz_label.musicbrainz_label_id AS musicbrainz_label_musicbrainz_id, \
                musicbrainz_label.discogs_label_id AS musicbrainz_label_discogs_id \
         FROM import_candidate_failure failure \
         LEFT JOIN import_candidate_artist_identity_conflict conflict \
             ON conflict.content_hash = failure.content_hash \
         LEFT JOIN artists discogs ON discogs.id = conflict.discogs_library_artist_id \
         LEFT JOIN artists musicbrainz ON musicbrainz.id = conflict.musicbrainz_library_artist_id \
         LEFT JOIN import_candidate_label_identity_conflict label_conflict \
             ON label_conflict.content_hash = failure.content_hash \
         LEFT JOIN labels discogs_label \
             ON discogs_label.id = label_conflict.discogs_library_label_id \
         LEFT JOIN labels musicbrainz_label \
             ON musicbrainz_label.id = label_conflict.musicbrainz_library_label_id \
         WHERE failure.content_hash = ?"
        ),
        [content_hash],
        |row| {
            let artist = match row.get::<_, Option<String>>("incoming_artist_name")? {
                None => None,
                Some(incoming_artist_name) => {
                    Some(IdentityConflict::Artist(ArtistIdentityConflict {
                        incoming_artist_name,
                        discogs_artist_id: row.get("incoming_discogs_artist_id")?,
                        musicbrainz_artist_id: row.get("incoming_musicbrainz_artist_id")?,
                        discogs_artist: existing_artist_from_row(row, "discogs")?,
                        musicbrainz_artist: existing_artist_from_row(row, "musicbrainz")?,
                    }))
                }
            };
            let label = match row.get::<_, Option<String>>("incoming_label_name")? {
                None => None,
                Some(incoming_label_name) => Some(IdentityConflict::Label(LabelIdentityConflict {
                    incoming_label_name,
                    discogs_label_id: row.get("incoming_discogs_label_id")?,
                    musicbrainz_label_id: row.get("incoming_musicbrainz_label_id")?,
                    discogs_label: existing_label_from_row(row, "discogs_label")?,
                    musicbrainz_label: existing_label_from_row(row, "musicbrainz_label")?,
                })),
            };
            Ok(ImportFailure {
                reason: failure_reason_from_row(row)?,
                failed_at: rfc3339_column(row, "failed_at")?,
                // The write that records a failure records one conflict at most.
                identity_conflict: artist.or(label),
            })
        },
    )
    .optional()
    .map_err(DbError::from)
}

/// Drop the failure the last import of `content_hash` left, and the identity
/// conflict that hangs off it.
pub(super) fn delete_failure_on(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_failure WHERE content_hash = ?", [content_hash])?;
    Ok(())
}

/// Record that an import of `content_hash` at `edit_revision` failed, inside
/// the caller's write.
fn save_failure_on(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    edit_revision: u64,
    failure: &ImportFailure,
) -> Result<(), DbError> {
    let edit_revision = to_i64(edit_revision, "candidate edit revision")?;
    let current_revision = sql
        .query_row(
            "SELECT edit_revision FROM import_candidate_state WHERE content_hash = ?",
            [content_hash],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| {
            DbError::Message("import failure has no current candidate state row".to_string())
        })?;
    if current_revision != edit_revision {
        return Err(DbError::Message(format!(
            "candidate file decisions changed from revision {edit_revision}"
        )));
    }
    let failed_at = failure.failed_at.to_rfc3339();
    sql.execute(
        "INSERT INTO import_candidate_failure \
             (content_hash, error, failed_at) \
         VALUES (?, ?, ?) \
         ON CONFLICT (content_hash) DO UPDATE SET \
             error = excluded.error, failed_at = excluded.failed_at",
        params![content_hash, failure.reason.detail, failed_at],
    )?;
    for table in
        ["import_candidate_artist_identity_conflict", "import_candidate_label_identity_conflict"]
    {
        sql.execute(&format!("DELETE FROM {table} WHERE content_hash = ?"), [content_hash])?;
    }
    match &failure.identity_conflict {
        None => {}
        Some(IdentityConflict::Artist(conflict)) => {
            sql.execute(
                "INSERT INTO import_candidate_artist_identity_conflict (\
                     content_hash, incoming_artist_name, discogs_artist_id, \
                     musicbrainz_artist_id, discogs_library_artist_id, \
                     musicbrainz_library_artist_id) VALUES (?, ?, ?, ?, ?, ?)",
                params![
                    content_hash,
                    conflict.incoming_artist_name,
                    conflict.discogs_artist_id,
                    conflict.musicbrainz_artist_id,
                    conflict.discogs_artist.artist_id,
                    conflict.musicbrainz_artist.artist_id,
                ],
            )?;
        }
        Some(IdentityConflict::Label(conflict)) => {
            sql.execute(
                "INSERT INTO import_candidate_label_identity_conflict (\
                     content_hash, incoming_label_name, discogs_label_id, \
                     musicbrainz_label_id, discogs_library_label_id, \
                     musicbrainz_library_label_id) VALUES (?, ?, ?, ?, ?, ?)",
                params![
                    content_hash,
                    conflict.incoming_label_name,
                    conflict.discogs_label_id,
                    conflict.musicbrainz_label_id,
                    conflict.discogs_label.label_id,
                    conflict.musicbrainz_label.label_id,
                ],
            )?;
        }
    }
    Ok(())
}

impl ImportTables {
    /// Record that an import of this candidate failed, so the pane still
    /// offers Retry after a relaunch.
    pub async fn save_import_candidate_failure(
        &self,
        content_hash: &str,
        edit_revision: u64,
        failure: &ImportFailure,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let failure = failure.clone();
        self.database
            .call(move |sql| save_failure_on(sql, &content_hash, edit_revision, &failure))
            .await
    }

    /// Record that the import `import_id` of the candidate at
    /// `candidate_key` failed, and end its claim on the candidate, in one
    /// write: no read sees the import ended without its failure, or the
    /// failure beside the import still going.
    pub(crate) async fn fail_candidate_import(
        &self,
        content_hash: &str,
        edit_revision: u64,
        failure: &ImportFailure,
        candidate_key: &crate::import::CandidateKey,
        import_id: &str,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let failure = failure.clone();
        let candidate_key = candidate_key.clone();
        let import_id = import_id.to_string();
        self.database
            .call(move |sql| {
                save_failure_on(sql, &content_hash, edit_revision, &failure)?;
                super::move_import_on(sql, &candidate_key, &import_id, None)
            })
            .await
    }
}
