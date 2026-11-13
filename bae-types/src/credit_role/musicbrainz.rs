//! How MusicBrainz states what an artist did.
//!
//! A relation names its type (`composer`, `vocal arranger`) and the
//! attributes that qualify it: modifiers (`additional`), a free-text `task`,
//! and the instruments or vocal kinds a performer or arranger plays or
//! sings. Each relationship type page is reproduced here name for name with
//! its reverse link phrase, the words MusicBrainz reads a relation with from
//! the artist's side; a type bae names becomes a [`CreditRoleKind`], any
//! other credit is stated in its phrase, and a type that names an artist
//! without crediting work on the thing is not a credit.

use super::{CreditRole, CreditRoleKind, CreditedAs, QualifierKind, RoleQualifier};
use std::collections::HashMap;

/// What bae makes of a relationship type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Treatment {
    /// A role bae names.
    Kind(CreditRoleKind),
    /// A credit in the words of the type's phrase.
    Stated,
    /// A relation naming an artist without crediting work on the thing.
    NotACredit,
    /// A heading of MusicBrainz's list that no relation carries.
    Heading,
}

/// A relationship type: its name, the reverse link phrase MusicBrainz reads
/// it with, and what bae makes of it.
type TypeRow = (&'static str, &'static str, Treatment);

use CreditRoleKind as R;
use Treatment::{Heading, Kind, NotACredit, Stated};

/// The artist-work relationship types, in the page's order.
const ARTIST_WORK: &[TypeRow] = &[
    ("authorship", "authorship", Heading),
    ("writer", "{additional} writer", Kind(R::Writer)),
    ("composer", "{additional} composer", Kind(R::Composer)),
    ("lyricist", "{additional} lyricist", Kind(R::Lyricist)),
    ("librettist", "{additional} librettist", Kind(R::Librettist)),
    ("revised by", "revised by", Kind(R::Reviser)),
    ("scriptwriter", "{additional} scriptwriter", Kind(R::Scriptwriter)),
    ("translator", "{additional} translator", Kind(R::Translator)),
    ("reconstructed by", "reconstructed by", Kind(R::Reconstructor)),
    ("arranger", "{additional} {associate} {co:co-}arranger", Kind(R::Arranger)),
    (
        "instrument arranger",
        "{additional} {associate} {instrument:%|instruments} {co:co-}arranger",
        Kind(R::Arranger),
    ),
    ("orchestrator", "{additional} orchestrator", Stated),
    (
        "vocal arranger",
        "{additional} {associate} {vocal:%|vocals} {co:co-}arranger",
        Kind(R::Arranger),
    ),
    ("adapter", "{additional} adapter", Stated),
    ("previous attribution", "previously attributed to", NotACredit),
    ("choreographer", "{assistant} choreographer", Stated),
    ("misc", "miscellaneous support", Heading),
    ("commissioned", "was commissioned by", NotACredit),
    ("dedication", "dedicated to", NotACredit),
    ("premiere", "premiered by", NotACredit),
    ("publishing", "{sub:sub-}publisher", Stated),
    ("named after artist", "named after artist", NotACredit),
    ("named after work", "inspired the name of artist", NotACredit),
];

/// A relation's attributes as MusicBrainz states them.
#[derive(Debug, Clone, Copy)]
pub struct Attributes<'a> {
    /// `attributes`: each attribute's name, in the relation's order.
    pub names: &'a [String],
    /// `attribute-credits`: the name a credit prints an attribute with.
    pub credits: &'a HashMap<String, String>,
    /// `attribute-values`: a free-text attribute's value.
    pub values: &'a HashMap<String, String>,
}

/// What an artist-work relation says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkReading {
    Credit(CreditedAs),
    /// The type names an artist without crediting work on the work.
    NotACredit,
    /// A type MusicBrainz's page does not list: credited in its own name.
    Unlisted(CreditedAs),
}

/// The credit an artist-work relation of `relation_type` states.
pub fn work_credit(relation_type: &str, attributes: Attributes<'_>) -> WorkReading {
    let Some((_, phrase, treatment)) =
        ARTIST_WORK.iter().find(|(name, _, _)| *name == relation_type)
    else {
        return WorkReading::Unlisted(CreditedAs {
            role: CreditRole::Stated(relation_type.to_string()),
            qualifiers: qualifiers(relation_type, attributes),
        });
    };
    let role = match treatment {
        Kind(kind) => CreditRole::Kind(*kind),
        Stated => CreditRole::Stated(phrase_words(phrase)),
        NotACredit => return WorkReading::NotACredit,
        Heading => {
            return WorkReading::Unlisted(CreditedAs {
                role: CreditRole::Stated(relation_type.to_string()),
                qualifiers: qualifiers(relation_type, attributes),
            })
        }
    };
    WorkReading::Credit(CreditedAs { role, qualifiers: qualifiers(relation_type, attributes) })
}

/// The qualifiers a relation's attributes state, in their order: the
/// modifiers bae names, a `task`'s value, the instruments of an instrument
/// arranger and the vocal kinds of a vocal arranger (`vocals` for one naming
/// none), and any other attribute by its name.
fn qualifiers(relation_type: &str, attributes: Attributes<'_>) -> Vec<RoleQualifier> {
    let mut qualifiers = Vec::new();
    let mut named_other = false;
    for name in attributes.names {
        if let Some(kind) = QualifierKind::from_key(name) {
            qualifiers.push(RoleQualifier::Kind(kind));
        } else if name == "task" {
            if let Some(value) = attributes.values.get(name) {
                qualifiers.push(RoleQualifier::Stated(value.clone()));
            }
        } else {
            let printed = attributes.credits.get(name).unwrap_or(name);
            qualifiers.push(RoleQualifier::Stated(printed.clone()));
            named_other = true;
        }
    }
    if relation_type == "vocal arranger" && !named_other {
        qualifiers.push(RoleQualifier::Stated("vocals".to_string()));
    }
    qualifiers
}

/// A reverse link phrase as plain words: each `{attribute}` or
/// `{attribute:text}` placeholder is taken out (the attribute is a qualifier
/// of the credit), and `{attribute:%|words}` reads as `words`.
pub fn phrase_words(phrase: &str) -> String {
    let mut words = String::new();
    let mut rest = phrase;
    while let Some(open) = rest.find('{') {
        words.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            rest = &rest[open..];
            break;
        };
        let placeholder = &rest[open + 1..open + close];
        if let Some((_, text)) = placeholder.split_once(':') {
            if let Some((_, alone)) = text.split_once("%|") {
                words.push_str(alone);
            }
        }
        rest = &rest[open + close + 1..];
    }
    words.push_str(rest);
    words.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
#[path = "musicbrainz_tests.rs"]
mod tests;
