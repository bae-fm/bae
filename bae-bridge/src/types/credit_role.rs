//! A credit's role as the apps word it: a label for the role and one for
//! each qualifier, from the catalog for what bae names and as the catalog
//! wrote it for the rest.

use super::pressing::{localized, verbatim};
use super::BridgeTermLabel;
use bae_types::credit_role::part::{Instrument, Part, Voice};
use bae_types::credit_role::{
    CreditRole, CreditRoleKind, CreditedAs, QualifierKind, RoleQualifier,
};

/// What an artist did, worded in parts: the role, then its qualifiers in the
/// order the catalog stated them. The apps join the qualifiers with their list
/// formatter and put them in parentheses after the role
/// (`core.credit.qualified`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCreditRole {
    pub role: BridgeTermLabel,
    pub qualifiers: Vec<BridgeTermLabel>,
}

/// One row of a release's Credits: a role, the artists who share it over the
/// same tracks, and those tracks.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCreditRow {
    pub role: BridgeCreditRole,
    pub artists: super::BridgeCreditLine,
    /// The position text of each track the row covers, in playing order;
    /// empty for a row covering every track of the release.
    pub tracks: Vec<String>,
}

impl BridgeCreditRow {
    pub(crate) fn from_core(row: bae_views::album_detail::CreditRow) -> Self {
        let bae_views::album_detail::CreditRow { role, artists, tracks } = row;
        Self {
            role: BridgeCreditRole::from_core(role),
            artists: super::BridgeCreditLine::from_core(artists),
            tracks,
        }
    }
}

/// The catalog key naming a role bae names.
pub(crate) fn credit_role_key(kind: CreditRoleKind) -> String {
    format!("core.credit.role.{}", kind.key())
}

/// The catalog key naming a qualifier bae names.
pub(crate) fn credit_qualifier_key(kind: QualifierKind) -> String {
    format!("core.credit.qualifier.{}", kind.key())
}

/// The catalog key naming an instrument on MusicBrainz's list.
pub(crate) fn credit_instrument_key(instrument: Instrument) -> String {
    format!("core.credit.instrument.{}", instrument.id())
}

/// The catalog key naming a voice on MusicBrainz's list.
pub(crate) fn credit_voice_key(voice: Voice) -> String {
    format!("core.credit.voice.{}", voice.id())
}

/// The catalog key naming a performer of no part the catalog names.
pub(crate) const PERFORMER_KEY: &str = "core.credit.role.performer";

impl BridgeCreditRole {
    pub(crate) fn from_core(credited: CreditedAs) -> Self {
        let CreditedAs { role, qualifiers } = credited;
        Self {
            role: match role {
                CreditRole::Kind(kind) => localized(&credit_role_key(kind)),
                CreditRole::Performer(None) => localized(PERFORMER_KEY),
                CreditRole::Performer(Some(Part::Instrument(instrument))) => {
                    localized(&credit_instrument_key(instrument))
                }
                CreditRole::Performer(Some(Part::Voice(voice))) => {
                    localized(&credit_voice_key(voice))
                }
                CreditRole::Performer(Some(Part::Words(words))) => verbatim(&words),
                CreditRole::Stated(words) => verbatim(&words),
            },
            qualifiers: qualifiers
                .into_iter()
                .map(|qualifier| match qualifier {
                    RoleQualifier::Kind(kind) => localized(&credit_qualifier_key(kind)),
                    RoleQualifier::Stated(words) => verbatim(&words),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
#[path = "credit_role_tests.rs"]
mod tests;
