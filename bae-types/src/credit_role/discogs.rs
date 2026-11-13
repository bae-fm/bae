//! How Discogs states what an artist did.
//!
//! A credit's role is one string: a comma list of roles, each a name from
//! Discogs's credits list with an optional bracketed qualifier that may hold
//! commas of its own (`Engineer [Assistant, Mixing]`, `Guitar, Vocals`). Each
//! role is a credit. A name bae has a role for becomes that role; an
//! instrument or voice becomes a performer of it; any other name is credited
//! as Discogs writes it. `discogs-credits.tsv` is Discogs's whole list as
//! captured, and `discogs-readings.tsv` states how bae reads each of its roles,
//! one row per captured role; a test compares the two.

use super::part::{Instrument, Part, Voice};
use super::{CreditRole, CreditRoleKind, CreditedAs, QualifierKind, RoleQualifier};
use std::sync::LazyLock;

/// What bae makes of a name on Discogs's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// A role bae names, with the qualifier the name itself states.
    Kind(CreditRoleKind, Option<QualifierKind>),
    /// A performer of the instrument on MusicBrainz's list the name is
    /// (given by its MusicBrainz name).
    Instrument(&'static str),
    /// A performer of the voice on MusicBrainz's list the name is (given by
    /// its MusicBrainz name), with the qualifier the name itself states.
    Voice(&'static str, Option<QualifierKind>),
    /// A performer of the instrument, voice or ensemble the name is, in
    /// Discogs's own word: bae's lists do not hold it.
    Named,
    /// A performer with no part: the name states none, or only a qualifier.
    Performer(Option<QualifierKind>),
    /// A role bae has no name for, credited as Discogs writes it.
    Stated,
}

/// Every role on Discogs's credits list, each under the heading Discogs lists
/// it under, with how bae reads it (`discogs-readings.tsv`).
static ROLES: LazyLock<Vec<(&'static str, &'static str, Reading)>> = LazyLock::new(|| {
    include_str!("discogs-readings.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(reading_row)
        .collect()
});

/// The row of `discogs-readings.tsv` a line states.
fn reading_row(line: &'static str) -> (&'static str, &'static str, Reading) {
    let fields: Vec<&'static str> = line.split('\t').collect();
    let [heading, role, how, target, qualifier] = fields[..] else {
        panic!("a readings line has five fields: {line}");
    };
    let qualifier = (!qualifier.is_empty()).then(|| {
        QualifierKind::from_key(qualifier)
            .unwrap_or_else(|| panic!("a qualifier bae names: {line}"))
    });
    let reading = match how {
        "kind" => Reading::Kind(
            CreditRoleKind::from_key(target).unwrap_or_else(|| panic!("a role bae names: {line}")),
            qualifier,
        ),
        "instrument" => Reading::Instrument(target),
        "voice" => Reading::Voice(target, qualifier),
        "performer" => Reading::Performer(qualifier),
        "named" => Reading::Named,
        "stated" => Reading::Stated,
        _ => panic!("a reading bae knows: {line}"),
    };
    (heading, role, reading)
}

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
        Some((_, Reading::Kind(kind, qualifier))) => (CreditRole::Kind(kind), qualifier),
        Some((_, Reading::Performer(qualifier))) => (CreditRole::Performer(None), qualifier),
        Some((listed, Reading::Instrument(name))) => (
            CreditRole::Performer(Some(
                Instrument::from_name(name)
                    .map_or_else(|| Part::Words(listed.to_string()), Part::Instrument),
            )),
            None,
        ),
        Some((listed, Reading::Voice(name, qualifier))) => (
            CreditRole::Performer(Some(
                Voice::from_name(name).map_or_else(|| Part::Words(listed.to_string()), Part::Voice),
            )),
            qualifier,
        ),
        Some((listed, Reading::Named)) => {
            (CreditRole::Performer(Some(Part::Words(listed.to_string()))), None)
        }
        Some((listed, Reading::Stated)) => (CreditRole::Stated(listed.to_string()), None),
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
