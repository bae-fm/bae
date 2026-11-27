//! A save's draft, written over the draft stored: rows that are the same are
//! left alone, and the draft row, its album artists and each track that stays
//! where it is are changed in place. The draft row is never deleted, so the
//! import list, whose triggers place the candidate after each row changes,
//! never reads the candidate as having no draft, no title or no artist on its
//! way from one draft to the next.

use super::draft_values::{insert_barcode_rows, insert_label_rows, store_album_artist_assignments};
use super::pane_rows::{insert_draft_body, insert_track_rows, store_track_row, EditRow};
use super::*;
use crate::{AudioFile, CandidateDraft, CandidateTrack, MetadataAuthor, TrackArtistAssignments};
use bae_store::WriteStatements as _;

sqlive_compose::write!(
    UpdateDraftRow(
        content_hash: &'p str,
        album_title: &'p str,
        album_year: &'p str,
        year: &'p str,
        country: Option<&'static str>,
        region: Option<&'static str>,
        media: &'p str,
        status: Option<&'static str>,
        packaging: Option<&'static str>,
        discogs_details: &'p str,
        author: &'static str,
        draft_blank: bool,
        draft_valid: bool,
    ),
    r#"
    UPDATE import_candidate_edit SET
        album_title = :album_title, album_year = :album_year, year = :year, country = :country,
        region = :region, media = :media, status = :status, packaging = :packaging,
        discogs_details = :discogs_details, author = :author, draft_blank = :draft_blank,
        draft_valid = :draft_valid
    WHERE content_hash = :content_hash
"#
);

sqlive_compose::write!(
    DeleteDraftLabels(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_edit_label WHERE content_hash = :content_hash
"#
);

sqlive_compose::write!(
    DeleteDraftBarcodes(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_edit_barcode WHERE content_hash = :content_hash
"#
);

/// Write `draft`, authored by `author`, over `stored`, which `stored_author`
/// wrote.
pub(super) fn store_draft(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    (stored, stored_author): (&CandidateDraft, MetadataAuthor),
    (draft, author): (&CandidateDraft, MetadataAuthor),
) -> Result<(), DbError> {
    let row = EditRow::of(draft, author);
    if EditRow::of(stored, stored_author) != row {
        sql.run(&UpdateDraftRowParams {
            content_hash,
            album_title: &row.form.album_title,
            album_year: &row.form.album_year,
            year: &row.form.year,
            country: row.form.facts.country,
            region: row.form.facts.region,
            media: &row.form.media,
            status: row.form.facts.status,
            packaging: row.form.facts.packaging,
            discogs_details: &row.form.facts.discogs_details,
            author: row.author,
            draft_blank: row.draft_blank,
            draft_valid: row.draft_valid,
        })?;
    }
    if stored.pressing.labels != draft.pressing.labels {
        sql.run(&DeleteDraftLabelsParams { content_hash })?;
        insert_label_rows(sql, content_hash, &draft.pressing.labels)?;
    }
    if stored.pressing.barcodes != draft.pressing.barcodes {
        sql.run(&DeleteDraftBarcodesParams { content_hash })?;
        insert_barcode_rows(sql, content_hash, &draft.pressing.barcodes)?;
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
    place: &'a bae_types::mediums::TrackPlace,
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
    roles: &'a [bae_catalogs::credits::CreditedRole],
    works: &'a [String],
}

impl<'a> TrackRows<'a> {
    fn of(track: &'a CandidateTrack) -> Self {
        Self { artists: &track.edit.artist_assignments, roles: &track.roles, works: &track.works }
    }
}

sqlive_compose::write!(
    DeleteTrackArtists(content_hash: &'p str, track_id: &'p str),
    r#"
    DELETE FROM import_candidate_track_artist_assignment
    WHERE content_hash = :content_hash AND track_id = :track_id
"#
);

sqlive_compose::write!(
    DeleteTrackRoles(content_hash: &'p str, track_id: &'p str),
    r#"
    DELETE FROM import_candidate_track_role
    WHERE content_hash = :content_hash AND track_id = :track_id
"#
);

sqlive_compose::write!(
    DeleteTrackWorks(content_hash: &'p str, track_id: &'p str),
    r#"
    DELETE FROM import_candidate_track_work
    WHERE content_hash = :content_hash AND track_id = :track_id
"#
);

/// The rows hanging off one track: its artists, its roles and its works.
fn delete_track_rows(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    track_id: &str,
) -> Result<(), DbError> {
    sql.run(&DeleteTrackArtistsParams { content_hash, track_id })?;
    sql.run(&DeleteTrackRolesParams { content_hash, track_id })?;
    sql.run(&DeleteTrackWorksParams { content_hash, track_id })?;
    Ok(())
}

/// The rows below the draft row besides its album artists: its mediums, its
/// tracks with what hangs off each, the roles on the release and the works.
/// The import list reads none of them.
#[derive(PartialEq)]
struct DraftBody<'a> {
    mediums: &'a [bae_types::mediums::ReleaseMedium],
    tracks: &'a [CandidateTrack],
    release_roles: &'a [bae_catalogs::credits::CreditedRole],
    works: &'a bae_catalogs::works::WorkGraph,
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

sqlive_compose::write!(
    DeleteDraftTracks(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_track WHERE content_hash = :content_hash
"#
);

sqlive_compose::write!(
    DeleteDraftWorks(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_work WHERE content_hash = :content_hash
"#
);

sqlive_compose::write!(
    DeleteDraftReleaseRoles(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_release_role WHERE content_hash = :content_hash
"#
);

sqlive_compose::write!(
    DeleteDraftMediums(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_medium WHERE content_hash = :content_hash
"#
);

/// Clear the draft's body: tracks first, which reference the mediums and the
/// works, each taking the rows that hang off it.
fn delete_draft_body(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.run(&DeleteDraftTracksParams { content_hash })?;
    sql.run(&DeleteDraftWorksParams { content_hash })?;
    sql.run(&DeleteDraftReleaseRolesParams { content_hash })?;
    sql.run(&DeleteDraftMediumsParams { content_hash })?;
    Ok(())
}

#[cfg(test)]
#[path = "draft_store_tests.rs"]
mod tests;
