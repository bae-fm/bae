//! A save's draft, written over the draft stored: rows that are the same are
//! left alone, and the draft row, its album artists and each track that stays
//! where it is are changed in place. The draft row is never deleted, so the
//! import list, whose triggers place the candidate after each row changes,
//! never reads the candidate as having no draft, no title or no artist on its
//! way from one draft to the next.

use super::pane_rows::{
    edit_row_values, insert_draft_body, insert_track_rows, store_album_artist_assignment,
    store_track_row,
};
use super::*;
use crate::import::{
    AudioFile, CandidateDraft, CandidateTrack, Credited, MetadataAuthor, TrackArtistAssignments,
};

/// Write `draft`, authored by `author`, over `stored`, which `stored_author`
/// wrote.
pub(super) fn store_draft(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    (stored, stored_author): (&CandidateDraft, MetadataAuthor),
    (draft, author): (&CandidateDraft, MetadataAuthor),
) -> Result<(), DbError> {
    let mut row = edit_row_values(content_hash, draft, author);
    if edit_row_values(content_hash, stored, stored_author) != row {
        // The key goes last, after the columns it sets.
        row.rotate_left(1);
        sql.execute(
            "UPDATE import_candidate_edit SET \
                 (album_title, album_year, year, labels, barcodes, country, region, media, \
                  status, packaging, discogs_details, author, draft_blank, draft_valid) = \
                 (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             WHERE content_hash = ?",
            rusqlite::params_from_iter(row),
        )?;
    }
    store_album_artist_assignments(
        sql,
        content_hash,
        &stored.album_artist_assignments,
        &draft.album_artist_assignments,
    )?;
    let (was, body) = (DraftBody::of(stored), DraftBody::of(draft));
    if was == body {
        return Ok(());
    }
    if was.same_tracks_and_mediums(&body) {
        // The tracks stay where they are, on the mediums they were on: each
        // row that changed is changed in place, and a track's artists, roles
        // or works that changed are written anew.
        for (position, (stored, track)) in stored.tracks.iter().zip(&draft.tracks).enumerate() {
            if TrackRow::of(stored) != TrackRow::of(track) {
                store_track_row(sql, content_hash, position, track)?;
            }
            if TrackRows::of(stored) != TrackRows::of(track) {
                delete_track_rows(sql, content_hash, &track.edit.id)?;
                insert_track_rows(sql, content_hash, track)?;
            }
        }
        return Ok(());
    }
    // Tracks come or go or change places, or the mediums or works do, which
    // only a new reading of the draft does. Tracks and works each hold a
    // position no two may share, so one row at a time cannot move them past
    // one another; the body is written anew.
    delete_draft_body(sql, content_hash)?;
    insert_draft_body(sql, content_hash, draft)
}

/// What a track's own row holds.
#[derive(PartialEq)]
struct TrackRow<'a> {
    title: &'a str,
    takes_album_artists: bool,
    place: &'a crate::mediums::TrackPlace,
    stated_length_ms: Option<u64>,
    file: &'a AudioFile,
}

impl<'a> TrackRow<'a> {
    fn of(track: &'a CandidateTrack) -> Self {
        Self {
            title: &track.edit.title,
            takes_album_artists: matches!(
                track.edit.artist_assignments,
                TrackArtistAssignments::AlbumArtists
            ),
            place: &track.place,
            stated_length_ms: track.stated_length_ms,
            file: &track.edit.file,
        }
    }
}

/// What the rows hanging off a track hold.
#[derive(PartialEq)]
struct TrackRows<'a> {
    artists: &'a TrackArtistAssignments,
    roles: &'a [crate::import::CreditedRole],
    works: &'a [String],
}

impl<'a> TrackRows<'a> {
    fn of(track: &'a CandidateTrack) -> Self {
        Self {
            artists: &track.edit.artist_assignments,
            roles: &track.roles,
            works: &track.works,
        }
    }
}

/// The rows hanging off one track: its artists, its roles and its works.
fn delete_track_rows(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    track_id: &str,
) -> Result<(), DbError> {
    for table in [
        "import_candidate_track_artist_assignment",
        "import_candidate_track_role",
        "import_candidate_track_work",
    ] {
        sql.execute(
            &format!("DELETE FROM {table} WHERE content_hash = ? AND track_id = ?"),
            params![content_hash, track_id],
        )?;
    }
    Ok(())
}

/// Each album artist that differs from the one stored at its position, in
/// place, and the stored ones past the last.
fn store_album_artist_assignments(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    stored: &[Credited],
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        if stored.get(position) != Some(assignment) {
            store_album_artist_assignment(sql, content_hash, position, assignment)?;
        }
    }
    if stored.len() > assignments.len() {
        sql.execute(
            "DELETE FROM import_candidate_album_artist_assignment \
             WHERE content_hash = ? AND position >= ?",
            params![content_hash, assignments.len() as i64],
        )?;
    }
    Ok(())
}

/// The rows below the draft row besides its album artists: its mediums, its
/// tracks with what hangs off each, the roles on the release and the works.
/// The import list reads none of them.
#[derive(PartialEq)]
struct DraftBody<'a> {
    mediums: &'a [crate::mediums::ReleaseMedium],
    tracks: &'a [CandidateTrack],
    release_roles: &'a [crate::import::CreditedRole],
    works: &'a crate::import::WorkGraph,
}

impl<'a> DraftBody<'a> {
    fn of(draft: &'a CandidateDraft) -> Self {
        Self {
            mediums: &draft.mediums,
            tracks: &draft.tracks,
            release_roles: &draft.release_roles,
            works: &draft.works,
        }
    }

    /// Whether `other` holds the same tracks in the same order, on the same
    /// mediums, with the same works and roles on the release: what a change to
    /// the tracks' own fields leaves alone.
    fn same_tracks_and_mediums(&self, other: &Self) -> bool {
        self.mediums == other.mediums
            && self.release_roles == other.release_roles
            && self.works == other.works
            && self.tracks.len() == other.tracks.len()
            && self
                .tracks
                .iter()
                .zip(other.tracks)
                .all(|(stored, track)| stored.edit.id == track.edit.id)
    }
}

/// Clear the draft's body: tracks first, which reference the mediums and the
/// works, each taking the rows that hang off it.
fn delete_draft_body(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    for table in [
        "import_candidate_track",
        "import_candidate_work",
        "import_candidate_release_role",
        "import_candidate_medium",
    ] {
        sql.execute(
            &format!("DELETE FROM {table} WHERE content_hash = ?"),
            [content_hash],
        )?;
    }
    Ok(())
}
