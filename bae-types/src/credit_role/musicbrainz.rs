//! How MusicBrainz states what an artist did.
//!
//! A relation names its type (`composer`, `vocal arranger`, `instrument`) and
//! the attributes that qualify it: modifiers (`additional`), a free-text
//! `task`, and the instruments or vocal kinds a performer or arranger plays
//! or sings. Each relationship type page is reproduced here name for name with
//! its reverse link phrase, the words MusicBrainz reads a relation with from
//! the artist's side; a type bae names becomes a [`CreditRoleKind`], a
//! performance type a performer of each instrument or voice it names, any
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
    /// A performer with no instrument or voice of their own to name.
    Performance,
    /// A performer of each instrument the relation names.
    Instrument,
    /// A performer of each voice the relation names.
    Vocal,
}

/// A relationship type: its name, the reverse link phrase MusicBrainz reads
/// it with, and what bae makes of it.
type TypeRow = (&'static str, &'static str, Treatment);

use CreditRoleKind as R;
use Treatment::{Heading, Instrument, Kind, NotACredit, Performance, Stated, Vocal};

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

/// The artist-recording relationship types, in the page's order.
const ARTIST_RECORDING: &[TypeRow] = &[
    ("performance", "performance", Heading),
    ("performer", "{additional} {guest} {solo} performer", Performance),
    ("instrument", "{additional} {guest} {solo} {instrument:%|instruments}", Instrument),
    ("vocal", "{additional} {guest} {solo} {vocal:%|vocals}", Vocal),
    ("performing orchestra", "{additional} orchestra", Stated),
    ("conductor", "{additional} {assistant} conductor", Stated),
    ("chorus master", "{additional} {assistant} chorus master", Stated),
    ("concertmaster", "{guest} concertmaster", Stated),
    ("audio director", "{assistant} audio director", Stated),
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
    ("remixes and compilations", "remixes and compilations", Heading),
    ("compiler", "compiler", Stated),
    ("mix-DJ", "DJ-mixer", Stated),
    ("remixer", "{additional} {assistant} remixer", Stated),
    ("samples from artist", "contains {additional} samples by", NotACredit),
    ("video", "video", Heading),
    ("video appearance", "visual appearances", Stated),
    ("video director", "{assistant} video director", Stated),
    ("animation", "{additional} animation", Stated),
    ("artwork", "{additional} artwork", Stated),
    ("design", "{additional} {assistant} design", Stated),
    ("graphic design", "{additional} graphic design", Stated),
    ("illustration", "{additional} illustration", Stated),
    ("choreographer", "{assistant} choreographer", Stated),
    ("cinematographer", "cinematographer", Stated),
    ("production", "production", Heading),
    (
        "producer",
        "{additional} {assistant} {associate} {co:co-}{executive:executive }producer",
        Kind(R::Producer),
    ),
    (
        "engineer",
        "{additional} {assistant} {associate} {co:co-}{executive:executive }engineer",
        Kind(R::Engineer),
    ),
    ("audio", "{additional} {assistant} {associate} {co:co-}audio engineer", Stated),
    ("mastering", "{additional} {assistant} {associate} {co:co-}mastering", Kind(R::Mastering)),
    ("sound", "{additional} {assistant} {associate} {co:co-}sound engineer", Stated),
    ("mix", "{additional} {assistant} {associate} {co:co-}mixer", Kind(R::Mixer)),
    ("recording", "{additional} {assistant} {associate} {co:co-}recording engineer", Stated),
    ("field recordist", "field recordings", Stated),
    ("programming", "{additional} {assistant} {associate} {instrument} programming", Stated),
    ("editor", "{additional} {assistant} {associate} {co:co-}editor", Stated),
    ("balance", "{additional} {assistant} {associate} {co:co-}balance engineer", Stated),
    ("sound effects", "sound effects", Stated),
    ("misc", "miscellaneous support", Heading),
    ("legal representation", "legal representation", Stated),
    ("phonographic copyright", "phonographic copyright (℗) by", Stated),
    ("video copyright", "video copyrighted (©) by", Stated),
    ("booking", "booking", Stated),
    ("artists and repertoire", "artist & repertoire support", Stated),
    ("creative direction", "{additional} creative direction", Stated),
    ("art direction", "{additional} art direction", Stated),
    ("design/illustration", "{additional} design/illustration", Stated),
    ("photography", "{additional} photography", Stated),
    ("instrument technician", "{instrument:%|instruments} technician", Stated),
    ("publishing", "publisher", Stated),
    ("dedication", "dedicated to", NotACredit),
];

/// The artist-release relationship types, in the page's order.
const ARTIST_RELEASE: &[TypeRow] = &[
    ("performance", "performance", Heading),
    ("performer", "{additional} {guest} {solo} performer", Performance),
    ("instrument", "{additional} {guest} {solo} {instrument:%|instruments}", Instrument),
    ("vocal", "{additional} {guest} {solo} {vocal:%|vocals}", Vocal),
    ("performing orchestra", "{additional} orchestra", Stated),
    ("conductor", "{additional} {assistant} conductor", Stated),
    ("chorus master", "{additional} {assistant} chorus master", Stated),
    ("concertmaster", "{guest} concertmaster", Stated),
    ("audio director", "{assistant} audio director", Stated),
    ("remixes and compilations", "remixes and compilations", Heading),
    ("compiler", "compiler", Stated),
    ("mix-DJ", "DJ-mixer {medium}", Stated),
    ("remixer", "{additional} {assistant} remixer", Stated),
    ("samples from artist", "contains {additional} samples by", NotACredit),
    ("authorship", "authorship", Heading),
    ("writer", "{additional} writer", Kind(R::Writer)),
    ("composer", "{additional} composer", Kind(R::Composer)),
    ("lyricist", "{additional} lyricist", Kind(R::Lyricist)),
    ("librettist", "{additional} librettist", Kind(R::Librettist)),
    ("translator", "{additional} translator", Kind(R::Translator)),
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
    ("video", "video", Heading),
    ("video director", "{assistant} video director", Stated),
    ("production", "production", Heading),
    (
        "producer",
        "{additional} {assistant} {associate} {co:co-}{executive:executive }producer",
        Kind(R::Producer),
    ),
    (
        "engineer",
        "{additional} {assistant} {associate} {co:co-}{executive:executive }engineer",
        Kind(R::Engineer),
    ),
    ("audio", "{additional} {assistant} {associate} {co:co-}audio engineer", Stated),
    (
        "mastering",
        "{additional} {assistant} {associate} {co:co-}{pre:pre-}{re}mastering",
        Kind(R::Mastering),
    ),
    ("lacquer cut", "lacquer cut", Stated),
    ("sound", "{additional} {assistant} {associate} {co:co-}sound engineer", Stated),
    ("mix", "{additional} {assistant} {associate} {co:co-}mixer", Kind(R::Mixer)),
    ("recording", "{additional} {assistant} {associate} {co:co-}recording engineer", Stated),
    ("field recordist", "field recordings", Stated),
    ("programming", "{additional} {assistant} {associate} {instrument} programming", Stated),
    ("editor", "{additional} {assistant} {associate} {co:co-}editor", Stated),
    ("balance", "{additional} {assistant} {associate} {co:co-}balance engineer", Stated),
    ("transfer", "{additional} {assistant} {associate} {co:co-}transferrer", Stated),
    ("sound effects", "sound effects", Stated),
    ("misc", "miscellaneous support", Heading),
    ("copyright", "copyrighted (©) by", Stated),
    ("phonographic copyright", "phonographic copyright (℗) by", Stated),
    ("booking", "booking", Stated),
    ("legal representation", "{additional} legal representation", Stated),
    ("licensor", "licensed from", Stated),
    ("production coordinator", "production coordinator", Stated),
    ("art direction", "{additional} {assistant} art direction", Stated),
    ("artwork", "{additional} artwork", Stated),
    ("design", "{additional} {assistant} design", Stated),
    ("graphic design", "{additional} graphic design", Stated),
    ("illustration", "{additional} illustration", Stated),
    ("design/illustration", "{additional} design/illustration", Stated),
    ("booklet editor", "booklet editor", Stated),
    ("photography", "{additional} photography", Stated),
    ("instrument technician", "{instrument:%|instruments} technician", Stated),
    ("publishing", "{sub:sub-}publisher", Stated),
    ("liner notes", "{additional} liner notes {translator}", Stated),
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

/// What a relation credits: the thing it relates the artist to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    Work,
    Recording,
    Release,
}

impl Subject {
    fn table(self) -> &'static [TypeRow] {
        match self {
            Self::Work => ARTIST_WORK,
            Self::Recording => ARTIST_RECORDING,
            Self::Release => ARTIST_RELEASE,
        }
    }
}

/// What an artist relation says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// The credits it states: one, or one for each instrument or voice a
    /// performer relation names.
    Credits(Vec<CreditedAs>),
    /// The type names an artist without crediting work on the thing.
    NotACredit,
    /// A type MusicBrainz's page does not list: credited in its own name.
    Unlisted(CreditedAs),
}

/// The credits an artist relation of `relation_type` to `subject` states.
pub fn credits(subject: Subject, relation_type: &str, attributes: Attributes<'_>) -> Reading {
    let listed = subject.table().iter().find(|(name, _, _)| *name == relation_type);
    let (phrase, treatment) = match listed {
        Some((_, phrase, treatment)) if *treatment != Heading => (phrase, *treatment),
        _ => {
            return Reading::Unlisted(CreditedAs {
                role: CreditRole::Stated(relation_type.to_string()),
                qualifiers: qualifiers(relation_type, attributes),
            })
        }
    };
    let role = match treatment {
        Kind(kind) => CreditRole::Kind(kind),
        Stated => CreditRole::Stated(phrase_words(phrase)),
        NotACredit => return Reading::NotACredit,
        Performance => CreditRole::Performer(None),
        Instrument | Vocal => return Reading::Credits(performers(treatment, attributes)),
        Heading => unreachable!("a heading is unlisted above"),
    };
    Reading::Credits(vec![CreditedAs { role, qualifiers: qualifiers(relation_type, attributes) }])
}

/// A performer of each instrument or voice a relation's attributes name
/// (the printed name where a credit prints one), each with the modifiers
/// the relation states. A relation naming none is a performer of no
/// instrument, and one of voices a performer of "vocals".
fn performers(treatment: Treatment, attributes: Attributes<'_>) -> Vec<CreditedAs> {
    let modifiers: Vec<RoleQualifier> = attributes
        .names
        .iter()
        .filter_map(|name| QualifierKind::from_key(name).map(RoleQualifier::Kind))
        .collect();
    let parts: Vec<&String> =
        attributes.names.iter().filter(|name| QualifierKind::from_key(name).is_none()).collect();
    if parts.is_empty() {
        let part = (treatment == Vocal).then(|| "vocals".to_string());
        return vec![CreditedAs { role: CreditRole::Performer(part), qualifiers: modifiers }];
    }
    parts
        .into_iter()
        .map(|name| CreditedAs {
            role: CreditRole::Performer(Some(attributes.credits.get(name).unwrap_or(name).clone())),
            qualifiers: modifiers.clone(),
        })
        .collect()
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
