//! The blank draft of a folder, which views build from what the folder's scan
//! stored of its files. Its views are `blank_draft_rows.sql`, which every
//! device's library holds.
//!
//! The scan stores the blank draft for a new state when the folder's files
//! hold no tags to draft from, and deleting this device's release of a folder
//! stores it again (`local_release_origin_gone_seeds_blank_draft`, in
//! `blank_draft_rows.sql`, which runs after `import_origin.sql` creates the
//! table it fires on). So every state of a folder this device did not import
//! has a draft row once the scan holds the folder's audio units.

use super::*;
use crate::{CandidateDraft, CandidateTrack, RawTrackEdit, TrackArtistAssignments};
use bae_store::WriteStatements as _;

coven::write!(
    SeedBlankDraft(content_hash: &'p str),
    r#"
    INSERT INTO blank_draft_seed (content_hash) VALUES (:content_hash)
"#
);

/// Store the blank draft and cover of the candidate `content_hash`, which
/// holds no draft row, through `blank_draft_seed`, the write the trigger on a
/// deleted release runs too. Stores nothing for a state the scan holds no
/// audio units of yet.
pub(super) fn seed_blank_draft_on(
    sql: &WriteContext<'_>,
    content_hash: &str,
) -> Result<(), DbError> {
    sql.run(&SeedBlankDraftParams { content_hash })?;
    Ok(())
}

// The blank draft of a candidate as its views hold it, with its mediums and
// tracks as JSON arrays in their order.
coven::read!(
    writer BlankDraftOf(content_hash: String) {
        struct StoredBlankTrack {
            track_id: String,
            medium: u16,
            track_number: u32,
            file_kind: String,
            file_id: String,
            sheet_id: Option<String>,
            slice_index: Option<u32>
        }
        album_title: String,
        mediums: json<Vec<u16>>,
        tracks: json<Vec<StoredBlankTrack>>
    },
    r#"
    SELECT edit.album_title,
        (SELECT json_group_array(medium.position ORDER BY medium.position)
         FROM blank_draft_medium AS medium
         WHERE medium.content_hash = :content_hash) AS mediums,
        (SELECT json_group_array(json_object('track_id', track.track_id,
                    'medium', track.medium, 'track_number', track.track_number,
                    'file_kind', track.file_kind, 'file_id', track.file_id,
                    'sheet_id', track.sheet_id, 'slice_index', track.slice_index)
                    ORDER BY track.position)
         FROM blank_draft_track AS track
         WHERE track.content_hash = :content_hash) AS tracks
    FROM blank_draft_edit AS edit
    WHERE edit.content_hash = :content_hash
"#
);

impl StoredBlankTrack {
    fn track(self) -> Result<CandidateTrack, DbError> {
        let file = match (self.file_kind.as_str(), self.sheet_id, self.slice_index) {
            ("standalone", None, None) => crate::AudioFile::Standalone { file_id: self.file_id },
            ("sheet_slice", Some(sheet_id), Some(index)) => {
                crate::AudioFile::SheetSlice { file_id: self.file_id, sheet_id, index }
            }
            (kind, sheet_id, index) => {
                return Err(DbError::unreadable(
                    "a blank draft track's file",
                    (kind.to_string(), sheet_id, index),
                ))
            }
        };
        let stated = |value: u32, what: &str| {
            std::num::NonZeroU32::new(value).ok_or_else(|| DbError::unreadable(what, value))
        };
        let place = bae_types::mediums::TrackPlace {
            medium: std::num::NonZeroU16::new(self.medium)
                .ok_or_else(|| DbError::unreadable("a blank draft track's medium", self.medium))?,
            side: std::num::NonZeroU8::MIN,
            number: stated(self.track_number, "a blank draft track's number")?,
        };
        Ok(CandidateTrack::unsourced(
            RawTrackEdit {
                id: self.track_id,
                title: String::new(),
                artist_assignments: TrackArtistAssignments::AlbumArtists,
                file,
            },
            place,
        ))
    }
}

impl crate::tables::ImportTables {
    /// The blank draft of the candidate `content_hash`: what clearing its
    /// metadata puts back. `None` when this device imported its files or the
    /// scan holds no audio units of them.
    pub(crate) async fn load_blank_draft(
        &self,
        content_hash: &str,
    ) -> Result<Option<CandidateDraft>, DbError> {
        let request = BlankDraftOfParams { content_hash: content_hash.to_string() };
        let Some(row) = self.store.read_rows(request).await?.pop() else {
            return Ok(None);
        };
        Ok(Some(CandidateDraft {
            album_title: row.album_title,
            album_artist_assignments: Vec::new(),
            album_year: String::new(),
            pressing: crate::RawPressingEdit::from_pressing(&bae_types::pressing::Pressing::blank()),
            mediums: row
                .mediums
                .iter()
                .map(|_| bae_types::mediums::ReleaseMedium::one_sided(None))
                .collect(),
            tracks: row
                .tracks
                .into_iter()
                .map(StoredBlankTrack::track)
                .collect::<Result<_, _>>()?,
            release_roles: Vec::new(),
            works: Default::default(),
        }))
    }
}
