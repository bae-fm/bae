//! The JSON a draft binds to the library's layout statement
//! (`bae_records::records::layout`): its mediums, its tracks' places and the
//! sides picked for its CUE sheets. The release editor binds an applied
//! catalog's draft, and the import pane its own draft.

use super::*;

/// One medium of a draft, as `:mediums` binds it.
#[derive(serde::Serialize)]
struct MediumJson {
    position: u16,
    carrier: Option<&'static str>,
    sides: u8,
}

/// Where a draft places a track, the length it lists for it and its playing
/// position, as `:places` binds it.
#[derive(serde::Serialize)]
struct PlaceJson<'a> {
    track_id: &'a str,
    medium: u16,
    side: u8,
    number: u32,
    stated_length_ms: Option<u64>,
    position: usize,
}

/// The side picked for a CUE sheet, as `:moves` binds it.
#[derive(serde::Serialize)]
struct MoveJson<'a> {
    sheet_file_id: &'a str,
    medium: u32,
    side: u32,
}

/// `value` as the JSON a statement binds.
pub(super) fn json(value: &impl serde::Serialize, what: &str) -> Result<String, DbError> {
    serde_json::to_string(value).map_err(|error| DbError::encode(what, error))
}

/// The mediums of `draft` as `:mediums` binds them; `None` binds none.
pub(super) fn mediums_json<Binding>(
    draft: Option<&crate::ReleaseDraft<Binding>>,
    what: &str,
) -> Result<String, DbError> {
    let mediums: Vec<_> = draft
        .map_or(&[][..], |draft| draft.mediums.as_slice())
        .iter()
        .enumerate()
        .map(|(index, medium)| MediumJson {
            position: bae_types::mediums::medium_position(index).get(),
            carrier: medium.carrier.map(bae_types::pressing::Medium::key),
            sides: medium.sides.get(),
        })
        .collect();
    json(&mediums, what)
}

/// The places of `draft`'s tracks, in playing order, as `:places` binds
/// them; `None` binds none.
pub(super) fn places_json<Binding>(
    draft: Option<&crate::ReleaseDraft<Binding>>,
    what: &str,
) -> Result<String, DbError> {
    let places: Vec<_> = draft
        .map_or(&[][..], |draft| draft.tracks.as_slice())
        .iter()
        .enumerate()
        .map(|(position, track)| PlaceJson {
            track_id: &track.edit.id,
            medium: track.place.medium.get(),
            side: track.place.side.get(),
            number: track.place.number.get(),
            stated_length_ms: track.stated_length_ms,
            position,
        })
        .collect();
    json(&places, what)
}

/// The sides picked for CUE sheets, each a sheet's file id with the side,
/// as `:moves` binds them.
pub(super) fn moves_json<'a>(
    moves: impl IntoIterator<Item = (&'a str, bae_types::mediums::MediumSide)>,
    what: &str,
) -> Result<String, DbError> {
    let moves: Vec<_> = moves
        .into_iter()
        .map(|(sheet_file_id, side)| MoveJson {
            sheet_file_id,
            medium: side.medium,
            side: side.side,
        })
        .collect();
    json(&moves, what)
}
