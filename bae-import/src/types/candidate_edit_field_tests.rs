use bae_types::pressing::{MediaCount, Medium, PressingFacts};

use super::PressingFactEdit;

fn facts(media: &[(Medium, u32)]) -> PressingFacts {
    PressingFacts {
        media: media.iter().map(|&(medium, count)| MediaCount { medium, count }).collect(),
        ..Default::default()
    }
}

/// A count of zero removes only the named carrier.
#[test]
fn a_medium_count_of_zero_removes_that_medium() {
    let mut held = facts(&[(Medium::Cd, 2), (Medium::Dvd, 1)]);
    PressingFactEdit::MediumCount { medium: Medium::Cd, count: 0 }.apply(&mut held);
    assert_eq!(held, facts(&[(Medium::Dvd, 1)]));
}

/// A new count replaces the named carrier's and keeps the others as they are.
#[test]
fn a_medium_count_changes_only_that_medium() {
    let mut held = facts(&[(Medium::Cd, 1), (Medium::Dvd, 1)]);
    PressingFactEdit::MediumCount { medium: Medium::Dvd, count: 3 }.apply(&mut held);
    assert_eq!(held, facts(&[(Medium::Cd, 1), (Medium::Dvd, 3)]));
}

/// A carrier the pressing lacks is added after the others, and removing one
/// it lacks changes nothing.
#[test]
fn a_medium_count_adds_a_missing_medium() {
    let mut held = facts(&[(Medium::Cd, 1)]);
    PressingFactEdit::MediumCount { medium: Medium::Dvd, count: 1 }.apply(&mut held);
    assert_eq!(held, facts(&[(Medium::Cd, 1), (Medium::Dvd, 1)]));
    PressingFactEdit::MediumCount { medium: Medium::Vinyl, count: 0 }.apply(&mut held);
    assert_eq!(held, facts(&[(Medium::Cd, 1), (Medium::Dvd, 1)]));
}
