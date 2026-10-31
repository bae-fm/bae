//! Persisted terminal import failures, including recoverable artist conflicts.

use super::*;
use crate::import::{ArtistIdentityConflict, ExistingArtist, ImportFailure, ImportFailureReason};

/// The diagnostic column read by both the list and the candidate pane.
pub(crate) const FAILURE_REASON_COLUMNS: &str = "failure.error AS failure_error";

pub(crate) fn failure_reason_from_row(row: &Row<'_>) -> rusqlite::Result<ImportFailureReason> {
    Ok(ImportFailureReason {
        detail: row.get("failure_error")?,
    })
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
                musicbrainz.discogs_artist_id AS musicbrainz_discogs_id \
         FROM import_candidate_failure failure \
         LEFT JOIN import_candidate_artist_identity_conflict conflict \
             ON conflict.content_hash = failure.content_hash \
         LEFT JOIN artists discogs ON discogs.id = conflict.discogs_library_artist_id \
         LEFT JOIN artists musicbrainz ON musicbrainz.id = conflict.musicbrainz_library_artist_id \
         WHERE failure.content_hash = ?"
        ),
        [content_hash],
        |row| {
            let conflict = match row.get::<_, Option<String>>("incoming_artist_name")? {
                None => None,
                Some(incoming_artist_name) => Some(ArtistIdentityConflict {
                    incoming_artist_name,
                    discogs_artist_id: row.get("incoming_discogs_artist_id")?,
                    musicbrainz_artist_id: row.get("incoming_musicbrainz_artist_id")?,
                    discogs_artist: existing_artist_from_row(row, "discogs")?,
                    musicbrainz_artist: existing_artist_from_row(row, "musicbrainz")?,
                }),
            };
            Ok(ImportFailure {
                reason: failure_reason_from_row(row)?,
                failed_at: rfc3339_column(row, "failed_at")?,
                artist_identity_conflict: conflict,
            })
        },
    )
    .optional()
    .map_err(DbError::from)
}

/// Drop the failure the last import of `content_hash` left, and the artist
/// conflict that hangs off it.
pub(super) fn delete_failure_on(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_failure WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

/// Record that an import of `content_hash` at `edit_revision` failed, inside
/// the caller's write.
fn save_failure_on(
    sql: &SqlContext<'_, '_>,
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
    sql.execute(
        "DELETE FROM import_candidate_artist_identity_conflict WHERE content_hash = ?",
        [content_hash],
    )?;
    if let Some(conflict) = &failure.artist_identity_conflict {
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
    Ok(())
}

impl Database {
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
        self.call(move |sql| save_failure_on(sql, &content_hash, edit_revision, &failure))
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
        self.call(move |sql| {
            save_failure_on(sql, &content_hash, edit_revision, &failure)?;
            super::move_import_on(sql, &candidate_key, &import_id, None)
        })
        .await
    }
}
