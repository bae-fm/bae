//! Persisted terminal import failures, including recoverable artist conflicts.

use super::*;
use crate::import::{ArtistIdentityConflict, ExistingArtist, ImportFailure, ImportFailureReason};

/// The columns [`failure_reason_from_row`] reads, for a query over
/// `import_candidate_failure failure` joined to `albums existing` on the
/// album the failure names.
pub(crate) const FAILURE_REASON_COLUMNS: &str = "failure.kind AS failure_kind, \
     failure.existing_album_id AS failure_album_id, \
     existing.title AS failure_album_title, \
     failure.error AS failure_error";

/// The join that reads the title of the album a failure names, as of now.
pub(crate) const FAILURE_ALBUM_JOIN: &str =
    "LEFT JOIN albums existing ON existing.id = failure.existing_album_id";

/// Why a stored import failed, with the named album's current title.
pub(crate) fn failure_reason_from_row(
    row: &Row<'_>,
) -> coven::rusqlite::Result<ImportFailureReason> {
    let kind: String = row.get("failure_kind")?;
    match kind.as_str() {
        "already_in_library" => Ok(ImportFailureReason::AlreadyInLibrary {
            album_id: row.get("failure_album_id")?,
            album_title: row.get("failure_album_title")?,
        }),
        "error" => Ok(ImportFailureReason::Error {
            detail: row.get("failure_error")?,
        }),
        other => Err(coven::rusqlite::Error::FromSqlConversionFailure(
            0,
            coven::rusqlite::types::Type::Text,
            format!("unknown import failure kind {other:?}").into(),
        )),
    }
}

fn existing_artist_from_row(
    row: &Row<'_>,
    prefix: &str,
) -> coven::rusqlite::Result<ExistingArtist> {
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
         {FAILURE_ALBUM_JOIN} \
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
pub(super) fn delete_failure_on(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_failure WHERE content_hash = ?",
        [content_hash],
    )?;
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
        let edit_revision = i64::try_from(edit_revision).map_err(|_| {
            DbError::Message(format!(
                "candidate edit revision {edit_revision} exceeds SQLite's integer range"
            ))
        })?;
        self.call(move |sql| {
            let current_revision = sql
                .query_row(
                    "SELECT edit_revision FROM import_candidate_state WHERE content_hash = ?",
                    [&content_hash],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .ok_or_else(|| {
                    DbError::Message(
                        "import failure has no current candidate state row".to_string(),
                    )
                })?;
            if current_revision != edit_revision {
                return Err(DbError::Message(format!(
                    "candidate file decisions changed from revision {edit_revision}"
                )));
            }
            let (kind, existing_album_id, error) = match &failure.reason {
                ImportFailureReason::AlreadyInLibrary { album_id, .. } => {
                    ("already_in_library", Some(album_id.as_str()), None)
                }
                ImportFailureReason::Error { detail } => ("error", None, Some(detail.as_str())),
            };
            let failed_at = failure.failed_at.to_rfc3339();
            sql.execute(
                "INSERT INTO import_candidate_failure \
                     (content_hash, kind, existing_album_id, error, failed_at) \
                 VALUES (?, ?, ?, ?, ?) \
                 ON CONFLICT (content_hash) DO UPDATE SET \
                     kind = excluded.kind, existing_album_id = excluded.existing_album_id, \
                     error = excluded.error, failed_at = excluded.failed_at",
                params![content_hash, kind, existing_album_id, error, failed_at],
            )?;
            sql.execute(
                "DELETE FROM import_candidate_artist_identity_conflict WHERE content_hash = ?",
                [&content_hash],
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
        })
        .await
    }
}
