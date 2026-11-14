//! The blank draft of a folder, read from what its scan stored of the files.
//! Its views are `blank_draft_rows.sql`, which every device's library holds.
//!
//! The scan seeds a new state with it, and deleting this device's release of
//! a folder seeds it again (`local_release_origin_gone_seeds_blank_draft`, in
//! `blank_draft_rows.sql`, which runs after `import_origin.sql` creates the
//! table it fires on), so no state of a folder this device did not import
//! lacks a draft row.

pub(super) const SQL: &str = include_str!("blank_draft_rows.sql");

use super::*;
use crate::{CandidateDraft, CandidateTrack, RawTrackEdit, TrackArtistAssignments};

/// Store the blank draft and cover of the candidate `content_hash`, which
/// holds no draft row. Stores nothing for a state the scan holds no audio
/// units of yet.
pub(super) fn seed_blank_draft_on(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "INSERT INTO import_candidate_edit \
             (content_hash, album_title, album_year, year, media, discogs_details, author, \
              draft_blank, draft_valid) \
         SELECT content_hash, album_title, album_year, year, media, discogs_details, author, \
                draft_blank, draft_valid \
         FROM blank_draft_edit WHERE content_hash = ?",
        [content_hash],
    )?;
    sql.execute(
        "INSERT INTO import_candidate_medium (content_hash, position, medium, side_count) \
         SELECT content_hash, position, medium, side_count \
         FROM blank_draft_medium WHERE content_hash = ?",
        [content_hash],
    )?;
    sql.execute(
        "INSERT INTO import_candidate_track \
             (content_hash, track_id, position, title, artist_assignment_kind, medium, side, \
              track_number, stated_length_ms, file_kind, file_id, sheet_id, slice_index) \
         SELECT content_hash, track_id, position, title, artist_assignment_kind, medium, side, \
                track_number, stated_length_ms, file_kind, file_id, sheet_id, slice_index \
         FROM blank_draft_track WHERE content_hash = ?",
        [content_hash],
    )?;
    sql.execute(
        "INSERT INTO import_candidate_cover (content_hash, kind, file_id) \
         SELECT content_hash, kind, file_id FROM blank_draft_cover WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

/// The blank draft of the candidate `content_hash`, for a write that starts
/// the draft over from the files. `None` when it has none: the scan holds no
/// audio units of it, or this device imported its files.
pub(crate) fn load_blank_draft_on(
    sql: &impl coven::SqlRead,
    content_hash: &str,
) -> Result<Option<CandidateDraft>, DbError> {
    let Some(album_title) = sql
        .query_row(
            "SELECT album_title FROM blank_draft_edit WHERE content_hash = ?",
            [content_hash],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    else {
        return Ok(None);
    };
    let mediums = sql.query(
        "SELECT position FROM blank_draft_medium WHERE content_hash = ? ORDER BY position",
        [content_hash],
        |row| row.get::<_, u16>(0),
    )?;
    let tracks = sql.query(
        "SELECT track_id, medium, track_number, file_kind, file_id, sheet_id, slice_index \
         FROM blank_draft_track WHERE content_hash = ? ORDER BY position",
        [content_hash],
        |row| {
            let (kind, file_id, sheet_id, index) = (
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<u32>>(6)?,
            );
            let file = match (kind.as_str(), sheet_id, index) {
                ("standalone", None, None) => crate::AudioFile::Standalone { file_id },
                ("sheet_slice", Some(sheet_id), Some(index)) => {
                    crate::AudioFile::SheetSlice { file_id, sheet_id, index }
                }
                _ => return Err(rusqlite::Error::InvalidQuery),
            };
            let place = bae_types::mediums::TrackPlace {
                medium: row.get(1)?,
                side: std::num::NonZeroU8::MIN,
                number: row.get(2)?,
            };
            Ok(CandidateTrack::unsourced(
                RawTrackEdit {
                    id: row.get(0)?,
                    title: String::new(),
                    artist_assignments: TrackArtistAssignments::AlbumArtists,
                    file,
                },
                place,
            ))
        },
    )?;
    Ok(Some(CandidateDraft {
        album_title,
        album_artist_assignments: Vec::new(),
        album_year: String::new(),
        pressing: crate::RawPressingEdit::from_pressing(&bae_types::pressing::Pressing::blank()),
        mediums: mediums
            .iter()
            .map(|_| bae_types::mediums::ReleaseMedium::one_sided(None))
            .collect(),
        tracks,
        release_roles: Vec::new(),
        works: Default::default(),
    }))
}

impl crate::tables::ImportTables {
    /// The blank draft of the candidate `content_hash`: what clearing its
    /// metadata puts back. `None` when this device imported its files or the
    /// scan holds no audio units of them.
    pub(crate) async fn load_blank_draft(
        &self,
        content_hash: &str,
    ) -> Result<Option<CandidateDraft>, DbError> {
        let content_hash = content_hash.to_string();
        self.store.read(move |sql| load_blank_draft_on(&sql, &content_hash)).await
    }
}
