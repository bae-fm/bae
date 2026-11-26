//! The JSON of an artist field's standing, which the reads that spread
//! `bae_records::records::artist_standing::ArtistFieldStanding` return, as the
//! artist field it decodes to: the field's own standing, and each credit with
//! how it stands to the library.

use super::*;
use crate::tables::draft_rows::StoredAssignment;
use crate::{ArtistField, ArtistStanding, ArtistsStanding, StandingCredit};
use bae_records::ExistingArtist;

/// One credit of a field with its artist's standing, as the standing fragment writes it.
#[derive(serde::Deserialize)]
struct StandingCreditJson {
    credit: StoredAssignment,
    kind: String,
    choices: Option<Vec<ExistingArtist>>,
}

/// One field as the standing fragment writes it.
#[derive(serde::Deserialize)]
pub(crate) struct ArtistFieldJson {
    kind: String,
    count: Option<u32>,
    credits: Vec<StandingCreditJson>,
}

impl ArtistFieldJson {
    /// The field the fragment wrote, or what in it the fragment does not write.
    pub(crate) fn field(self) -> Result<ArtistField, DbError> {
        let unreadable = |stated: String| DbError::unreadable("an artist field's standing", stated);
        let standing = match (self.kind.as_str(), self.count) {
            ("library", None) => ArtistsStanding::Library,
            ("new", None) => ArtistsStanding::New,
            ("some_new", Some(count)) => ArtistsStanding::SomeNew { count },
            ("choose", Some(choices)) => ArtistsStanding::Choose { choices },
            ("some_to_choose", Some(count)) => ArtistsStanding::SomeToChoose { count },
            (kind, count) => {
                return Err(unreadable(format!("a field standing {kind} of count {count:?}")))
            }
        };
        let credits = self
            .credits
            .into_iter()
            .map(|credit| {
                let standing = match (credit.kind.as_str(), credit.choices) {
                    ("library", None) => ArtistStanding::Library,
                    ("new", None) => ArtistStanding::New,
                    ("ambiguous", Some(choices)) => ArtistStanding::Choose { choices },
                    (kind, choices) => {
                        return Err(unreadable(format!(
                            "an artist standing {kind} {} choices",
                            if choices.is_some() { "with" } else { "without" }
                        )))
                    }
                };
                Ok(StandingCredit { credited: credit.credit.credited()?, standing })
            })
            .collect::<Result<_, DbError>>()?;
        Ok(ArtistField { standing, credits })
    }
}

/// The field the fragment wrote as `json`.
pub(crate) fn artist_field_of_json(json: &str) -> Result<ArtistField, DbError> {
    let stated: ArtistFieldJson =
        serde_json::from_str(json).map_err(|source| DbError::UnreadableJson {
            what: "an artist field".to_string(),
            json: json.to_string(),
            source,
        })?;
    stated.field()
}

#[cfg(test)]
#[path = "artist_standings_tests.rs"]
mod tests;
