//! Laying a reading's values onto a draft: a candidate's, against the
//! folder's audio units, or a library release's, against its tracks. The
//! library editor's reset projects the editable ones (`user_edit`).

use crate::{
    AudioFile, CandidateDraft, Credited, RawPressingEdit, RawTrackEdit, ReleaseUserEdit,
    TrackArtistAssignments, TrackUserEdit,
};
use bae_catalogs::credits::SourceCredit;
use bae_catalogs::release_values::{ReleaseValues, TrackCredits};

/// Who a reading credits a track to, as the draft assigns it.
fn track_assignments(credits: &TrackCredits) -> TrackArtistAssignments {
    match credits {
        TrackCredits::AlbumArtists => TrackArtistAssignments::AlbumArtists,
        TrackCredits::Own(credits) => {
            TrackArtistAssignments::Explicit(credited_list(credits.iter()))
        }
    }
}

/// The draft's credit list for `credits`, as stored (see
/// [`Credited::normalized_list`]).
fn credited_list<'a>(credits: impl Iterator<Item = &'a SourceCredit>) -> Vec<Credited> {
    Credited::normalized_list(credits.cloned().map(Credited::of_source).collect())
}

/// The editable fields, as the library editor's form is seeded with them.
pub(crate) fn user_edit(values: &ReleaseValues) -> ReleaseUserEdit {
    ReleaseUserEdit {
        album_title: values.album_title.clone(),
        album_artist_assignments: album_assignments(values),
        album_year: values.album_year,
        pressing: values.pressing.clone(),
        tracks: values
            .tracks
            .iter()
            .map(|track| TrackUserEdit {
                title: track.title.clone(),
                artist_assignments: track_assignments(&track.credits),
                file: None,
            })
            .collect(),
    }
}

/// Every value, laid onto the folder's audio `units`: track `i` plays
/// unit `i`, under the draft's row identity for that place.
///
/// The reader laid the source out against these units, so they are as
/// many as its tracks.
pub(crate) fn candidate_draft(values: ReleaseValues, units: Vec<AudioFile>) -> CandidateDraft {
    let tracks = units
        .into_iter()
        .enumerate()
        .map(|(position, unit)| (crate::pane::candidate_track_id(position), unit))
        .collect();
    draft(values, tracks)
}

/// Every value, laid onto a library release's tracks in order: track `i`
/// is track row `i`.
pub fn library_draft(values: ReleaseValues, track_ids: Vec<String>) -> crate::LibraryDraft {
    let tracks = track_ids.into_iter().map(|id| (id.clone(), crate::LibraryTrack(id))).collect();
    draft(values, tracks)
}

/// Every value, laid onto `tracks` in order: each the draft row identity
/// and what that track is bound to. The reader laid the source out
/// against these, so they are as many as its tracks.
fn draft<Binding>(
    values: ReleaseValues,
    tracks: Vec<(String, Binding)>,
) -> crate::ReleaseDraft<Binding> {
    assert_eq!(
        values.tracks.len(),
        tracks.len(),
        "a reading lays one track on each track it was read against"
    );
    let album_artist_assignments = album_assignments(&values);
    let tracks = values
        .tracks
        .into_iter()
        .zip(tracks)
        .map(|(track, (id, file))| crate::DraftTrack {
            edit: RawTrackEdit {
                id,
                title: track.title,
                artist_assignments: track_assignments(&track.credits),
                file,
            },
            place: track.place,
            stated_length_ms: track.stated_length_ms,
            roles: track.roles,
            works: track.works,
        })
        .collect();
    crate::ReleaseDraft {
        album_title: values.album_title,
        album_artist_assignments,
        album_year: values.album_year.map(|year| year.to_string()).unwrap_or_default(),
        pressing: RawPressingEdit::from_pressing(&values.pressing),
        mediums: values.mediums,
        tracks,
        release_roles: values.release_roles,
        works: values.works,
    }
}

/// The album credits as assignments.
fn album_assignments(values: &ReleaseValues) -> Vec<Credited> {
    credited_list(values.album_credits.iter())
}

/// The rows a reading's draft commits as: the reading laid onto one loose
/// file per track and built into rows the way the import builds them. What a
/// reader's test checks a reading's rows with.
#[cfg(test)]
pub(crate) fn committed_rows(values: ReleaseValues) -> bae_records::ReleaseRows {
    let units = (0..values.tracks.len())
        .map(|index| AudioFile::Standalone { file_id: format!("{index:02}.flac") })
        .collect();
    let draft = candidate_draft(values, units);
    let clock = coven::FixedClock::new(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .expect("a fixed instant")
            .with_timezone(&chrono::Utc)
            .into(),
    );
    crate::assemble::assemble_release(&draft, &clock, &coven::SequentialIds::new())
        .expect("the reading's draft commits")
        .parsed
}

/// The library artist each of `parsed`'s artist rows becomes when its import
/// commits into an empty library, by row id: which credits the commit makes
/// one artist.
#[cfg(test)]
pub(crate) async fn committed_artists(
    parsed: &bae_records::ReleaseRows,
) -> std::collections::HashMap<String, String> {
    parsed
        .artists
        .iter()
        .map(|artist| artist.id.clone())
        .zip(bae_records::resolved_in_an_empty_library(&parsed.artists).await)
        .collect()
}

/// [`place_labels`](bae_catalogs::release_values::place_labels) of an import's track rows.
#[cfg(test)]
pub(crate) fn row_labels(rows: &bae_records::ReleaseRows) -> Vec<String> {
    let mediums: Vec<_> = rows.mediums.iter().map(|medium| medium.medium).collect();
    bae_catalogs::release_values::place_labels(
        &mediums,
        rows.tracks.iter().map(|track| {
            let medium = rows
                .mediums
                .iter()
                .find(|medium| medium.id == track.medium_id)
                .expect("a track row's medium is one of the release's");
            bae_types::mediums::TrackPlace {
                medium: medium.position,
                side: track.side,
                number: track.track_number,
            }
        }),
    )
}

#[cfg(test)]
#[path = "reading_draft_tests.rs"]
mod tests;
