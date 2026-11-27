//! The credits a release's detail lists, as
//! [`ReleaseCredits`](super::credit_fragments::ReleaseCredits) writes them:
//! by role, the artists who hold a role over the same tracks as one row.

use super::*;

/// One place of a credit row's tracks, as the statement spells it in JSON.
#[derive(serde::Deserialize)]
struct PlaceJson {
    kind: String,
    ordinal: u32,
    number: u32,
}

/// One credit row, as the statement spells it in JSON.
#[derive(serde::Deserialize)]
struct RowJson {
    role: String,
    role_words: Option<String>,
    qualifiers: String,
    artists: Vec<String>,
    tracks: Option<Vec<PlaceJson>>,
}

/// The release's credit rows, in the order it lists them, as
/// [`ReleaseCredits`](super::credit_fragments::ReleaseCredits) wrote them
/// into its document `json`.
pub(super) fn credit_rows_of_json(json: &str) -> Result<Vec<DbCreditRow>, DbError> {
    const DOCUMENT: &str = "a release's credits";
    let rows: Vec<RowJson> = serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json(DOCUMENT, json, error))?;
    rows.into_iter()
        .map(|credit| {
            Ok(DbCreditRow {
                role: bae_types::credit_role::CreditedAs::from_columns(
                    &credit.role,
                    credit.role_words,
                    &credit.qualifiers,
                )
                .map_err(|error| DbError::unreadable(DOCUMENT, error.to_string()))?,
                artists: credit.artists,
                tracks: credit
                    .tracks
                    .unwrap_or_default()
                    .into_iter()
                    .map(|place| {
                        super::release_tracks::track_position_of(
                            &place.kind,
                            place.ordinal,
                            place.number,
                        )
                        .map_err(|reason| DbError::unreadable(DOCUMENT, reason))
                    })
                    .collect::<Result<_, DbError>>()?,
            })
        })
        .collect()
}
