//! How Discogs states what an artist did.
//!
//! A credit's role is one string: a comma list of roles, each a name from
//! Discogs's credits list with an optional bracketed qualifier that may hold
//! commas of its own (`Engineer [Assistant, Mixing]`, `Guitar, Vocals`). Each
//! role is a credit. A name bae has a role for becomes that role; an
//! instrument or voice becomes a performer of it; any other name is credited
//! as Discogs writes it. The names bae reads are reproduced here name for name
//! with the heading Discogs lists them under, and a test compares them with
//! the captured list under `test-fixtures/credit-vocabulary/`.

use super::part::{Instrument, Part, Voice};
use super::{CreditRole, CreditRoleKind, CreditedAs, QualifierKind, RoleQualifier};

/// What bae makes of a name on Discogs's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// A role bae names, with the qualifier the name itself states.
    Kind(CreditRoleKind, Option<QualifierKind>),
    /// A performer of the instrument on MusicBrainz's list the name is
    /// (given by its MusicBrainz name).
    Instrument(&'static str),
    /// A performer of the voice on MusicBrainz's list the name is (given by
    /// its MusicBrainz name).
    Voice(&'static str),
    /// A performer of the instrument, voice or ensemble the name is, in
    /// Discogs's own word: bae's lists do not hold it.
    Named,
    /// A performer with no part: the name states none.
    Performer,
    /// A role bae has no name for, credited as Discogs writes it.
    Stated,
}

use CreditRoleKind as R;
use Reading::{Instrument as Plays, Kind, Named, Performer, Stated, Voice as Sings};

/// The names bae reads, each under the heading Discogs lists it under.
const ROLES: &[(&str, &str, Reading)] = &[
    ("Writing & Arrangement", "Written-By", Kind(R::Writer, None)),
    ("Writing & Arrangement", "Adapted By", Stated),
    ("Writing & Arrangement", "Arranged By", Kind(R::Arranger, None)),
    ("Writing & Arrangement", "Beats", Stated),
    ("Writing & Arrangement", "Cadenza", Stated),
    ("Writing & Arrangement", "Composed By", Kind(R::Composer, None)),
    ("Writing & Arrangement", "Concept By", Stated),
    ("Writing & Arrangement", "Copyist", Stated),
    ("Writing & Arrangement", "Created By", Stated),
    ("Writing & Arrangement", "Instrumentation By", Stated),
    ("Writing & Arrangement", "Libretto By", Kind(R::Librettist, None)),
    ("Writing & Arrangement", "Lyrics By", Kind(R::Lyricist, None)),
    ("Writing & Arrangement", "Music By", Kind(R::Composer, None)),
    ("Writing & Arrangement", "Musical Assistance", Stated),
    ("Writing & Arrangement", "Orchestrated By", Stated),
    ("Writing & Arrangement", "Programmed By", Stated),
    ("Writing & Arrangement", "Score", Stated),
    ("Writing & Arrangement", "Score Editor", Stated),
    ("Writing & Arrangement", "Sequenced By", Stated),
    ("Writing & Arrangement", "Songwriter", Kind(R::Writer, None)),
    ("Writing & Arrangement", "Sound Designer", Stated),
    ("Writing & Arrangement", "Transcription By", Stated),
    ("Writing & Arrangement", "Translated By", Kind(R::Translator, None)),
    ("Writing & Arrangement", "Words By", Kind(R::Lyricist, None)),
    ("Writing & Arrangement", "Music Consultant", Stated),
    ("Production", "Co-producer", Kind(R::Producer, Some(QualifierKind::Co))),
    ("Production", "Collected By", Stated),
    ("Production", "Commissioned By", Stated),
    ("Production", "Compilation Producer", Stated),
    ("Production", "Compiled By", Stated),
    ("Production", "Curated By", Stated),
    ("Production", "Editor", Stated),
    ("Production", "Executive-Producer", Kind(R::Producer, Some(QualifierKind::Executive))),
    ("Production", "Post Production", Stated),
    ("Production", "Producer", Kind(R::Producer, None)),
    ("Production", "Recording Supervisor", Stated),
    ("Production", "Reissue Producer", Stated),
    ("Production", "Research", Stated),
    ("Production", "Supervised By", Stated),
    ("Technical", "Authoring", Stated),
    ("Technical", "Crew", Stated),
    ("Technical", "DAW", Stated),
    ("Technical", "Direct Metal Mastering By", Stated),
    ("Technical", "Edited By", Stated),
    ("Technical", "Engineer", Kind(R::Engineer, None)),
    ("Technical", "Equipment", Stated),
    ("Technical", "Field Recording", Stated),
    ("Technical", "Instrument Builder", Stated),
    ("Technical", "Lacquer Cut By", Stated),
    ("Technical", "Lathe Cut By", Stated),
    ("Technical", "Lathe Designer", Stated),
    ("Technical", "Luthier", Stated),
    ("Technical", "Mastered By", Kind(R::Mastering, None)),
    ("Technical", "Mixed By", Kind(R::Mixer, None)),
    ("Technical", "Overdubbed By", Stated),
    ("Technical", "Plated By", Stated),
    ("Technical", "Recorded By", Stated),
    ("Technical", "Remastered By", Kind(R::Mastering, Some(QualifierKind::Re))),
    ("Technical", "Restoration", Stated),
    ("Technical", "Tape Op", Stated),
    ("Technical", "Technician", Stated),
    ("Technical", "Tracking By", Stated),
    ("Technical", "Transferred By", Stated),
    ("Technical", "Tuner", Stated),
    ("Acting, Literary & Spoken", "Text By", Stated),
    ("Acting, Literary & Spoken", "Script By", Kind(R::Scriptwriter, None)),
    ("Acting, Literary & Spoken", "Liner Notes", Stated),
    ("Instruments", "Performer", Performer),
    ("Instruments", "Musician", Performer),
    ("Instruments", "Orchestra", Named),
    ("Instruments", "Band", Named),
    ("Instruments", "Ensemble", Named),
    ("Instruments", "Soloist", Named),
    ("Instruments", "Bass", Plays("bass guitar")),
    ("Instruments", "Drums", Plays("drums (drum set)")),
    ("Instruments", "Percussion", Plays("percussion")),
    ("Instruments", "Guitar", Plays("guitar")),
    ("Instruments", "Keyboards", Plays("keyboard")),
    ("Instruments", "Strings", Plays("strings")),
    ("Instruments", "Violin", Plays("violin")),
    ("Instruments", "Piano", Plays("piano")),
    ("Instruments", "Electric Piano", Plays("electric piano")),
    ("Instruments", "Organ", Plays("organ")),
    ("Vocals", "Vocals", Sings("vocal")),
    ("Vocals", "Backing Vocals", Sings("background vocals")),
    ("Unlinked", "Artwork By", Stated),
    ("Unlinked", "Photography", Stated),
    ("Unlinked", "Executive Producer", Kind(R::Producer, Some(QualifierKind::Executive))),
    ("Unlinked", "Other", Stated),
    ("Unlinked", "Written By", Kind(R::Writer, None)),
];

/// The credits a Discogs role string states, one per role it lists.
pub fn credits(role: &str) -> Vec<CreditedAs> {
    split_roles(role).into_iter().map(|item| credit(&item)).collect()
}

/// `role` split at the commas outside brackets, each part trimmed; empty
/// parts left out.
fn split_roles(role: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for c in role.chars() {
        match c {
            '[' => {
                depth += 1;
                current.push(c);
            }
            ']' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            ',' if depth == 0 => items.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    items.push(current);
    items.into_iter().map(|item| item.trim().to_string()).filter(|item| !item.is_empty()).collect()
}

/// The credit one `Name [qualifier]` item states.
fn credit(item: &str) -> CreditedAs {
    let (name, bracket) = match item.find('[') {
        Some(open) => {
            let inner = item[open + 1..].trim_end().trim_end_matches(']').trim();
            (item[..open].trim(), Some(inner).filter(|inner| !inner.is_empty()))
        }
        None => (item, None),
    };
    let (role, stated_qualifier) = match ROLES
        .iter()
        .find(|(_, listed, _)| listed.eq_ignore_ascii_case(name))
        .map(|(_, listed, reading)| (*listed, *reading))
    {
        Some((_, Kind(kind, qualifier))) => (CreditRole::Kind(kind), qualifier),
        Some((_, Performer)) => (CreditRole::Performer(None), None),
        Some((listed, Plays(name))) => (
            CreditRole::Performer(Some(
                Instrument::from_name(name)
                    .map_or_else(|| Part::Words(listed.to_string()), Part::Instrument),
            )),
            None,
        ),
        Some((listed, Sings(name))) => (
            CreditRole::Performer(Some(
                Voice::from_name(name).map_or_else(|| Part::Words(listed.to_string()), Part::Voice),
            )),
            None,
        ),
        Some((listed, Named)) => {
            (CreditRole::Performer(Some(Part::Words(listed.to_string()))), None)
        }
        Some((listed, Stated)) => (CreditRole::Stated(listed.to_string()), None),
        None => (CreditRole::Stated(name.to_string()), None),
    };
    let mut qualifiers: Vec<RoleQualifier> =
        stated_qualifier.into_iter().map(RoleQualifier::Kind).collect();
    if let Some(bracket) = bracket {
        qualifiers.push(match QualifierKind::from_key(&bracket.to_ascii_lowercase()) {
            Some(kind) => RoleQualifier::Kind(kind),
            None => RoleQualifier::Stated(bracket.to_string()),
        });
    }
    CreditedAs { role, qualifiers }
}

#[cfg(test)]
#[path = "discogs_tests.rs"]
mod tests;
