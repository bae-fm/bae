//! Persisted terminal import failures, including recoverable artist conflicts.

use super::*;
use crate::import::{ArtistIdentityConflict, ExistingArtist, ImportFailure};

/// The failure the last import of `only` left, or every candidate's.
pub(super) fn load_failures_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, ImportFailure>, DbError> {
    let rows = sql.query(
        "SELECT failure.content_hash, failure.error, failure.failed_at, \
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
         WHERE :only IS NULL OR failure.content_hash = :only",
        named_params! { ":only": only },
        |row| {
            let conflict = match row.get::<_, Option<String>>("incoming_artist_name")? {
                None => None,
                Some(incoming_artist_name) => Some(ArtistIdentityConflict {
                    incoming_artist_name,
                    discogs_artist_id: row.get("incoming_discogs_artist_id")?,
                    musicbrainz_artist_id: row.get("incoming_musicbrainz_artist_id")?,
                    discogs_artist: ExistingArtist {
                        artist_id: row.get("discogs_id")?,
                        name: row.get("discogs_name")?,
                        sort_name: row.get("discogs_sort_name")?,
                        musicbrainz_artist_id: row.get("discogs_musicbrainz_id")?,
                        discogs_artist_id: row.get("discogs_discogs_id")?,
                    },
                    musicbrainz_artist: ExistingArtist {
                        artist_id: row.get("musicbrainz_id")?,
                        name: row.get("musicbrainz_name")?,
                        sort_name: row.get("musicbrainz_sort_name")?,
                        musicbrainz_artist_id: row.get("musicbrainz_musicbrainz_id")?,
                        discogs_artist_id: row.get("musicbrainz_discogs_id")?,
                    },
                }),
            };
            Ok((
                row.get::<_, String>("content_hash")?,
                row.get::<_, String>("error")?,
                rfc3339_column(row, "failed_at")?,
                conflict,
            ))
        },
    )?;
    Ok(rows
        .into_iter()
        .map(|(content_hash, error, failed_at, conflict)| {
            let failure = match conflict {
                Some(conflict) => ImportFailure::ArtistIdentityConflict {
                    error,
                    failed_at,
                    conflict,
                },
                None => ImportFailure::Error { error, failed_at },
            };
            (content_hash, failure)
        })
        .collect())
}

impl Database {
    /// Record that an import of this candidate failed, so the pane still
    /// offers Retry after a relaunch.
    ///
    /// The anchor row is created when nothing has identified or picked the
    /// candidate: an import driven straight from a command has no pick behind
    /// it, and the failure is still a fact about those bytes.
    pub async fn save_import_candidate_failure(
        &self,
        content_hash: &str,
        folder_path: &str,
        edit_revision: u64,
        failure: &ImportFailure,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let folder_path = folder_path.to_string();
        let failure = failure.clone();
        let edit_revision = i64::try_from(edit_revision).map_err(|_| {
            DbError::Message(format!(
                "candidate edit revision {edit_revision} exceeds SQLite's integer range"
            ))
        })?;
        self.call(move |sql| {
            let error = failure.error();
            let failed_at = failure.failed_at().to_rfc3339();
            sql.execute(
                "INSERT INTO import_candidate_state (content_hash, folder_path, edit_revision) \
                 VALUES (?, ?, ?) ON CONFLICT (content_hash) DO NOTHING",
                params![content_hash, folder_path, edit_revision],
            )?;
            sql.execute(
                "INSERT INTO import_candidate_failure (content_hash, error, failed_at) \
                 VALUES (?, ?, ?) \
                 ON CONFLICT (content_hash) DO UPDATE SET \
                     error = excluded.error, failed_at = excluded.failed_at",
                params![content_hash, error, failed_at],
            )?;
            sql.execute(
                "DELETE FROM import_candidate_artist_identity_conflict WHERE content_hash = ?",
                [&content_hash],
            )?;
            if let ImportFailure::ArtistIdentityConflict { conflict, .. } = &failure {
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
