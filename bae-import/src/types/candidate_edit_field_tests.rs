use bae_types::pressing::{DiscogsDetail, MediaCount, Medium, PressingFacts};

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
    PressingFactEdit::MediumCount(MediaCount { medium: Medium::Cd, count: 0 }).apply(&mut held);
    assert_eq!(held, facts(&[(Medium::Dvd, 1)]));
}

/// A new count replaces the named carrier's and keeps the others as they are.
#[test]
fn a_medium_count_changes_only_that_medium() {
    let mut held = facts(&[(Medium::Cd, 1), (Medium::Dvd, 1)]);
    PressingFactEdit::MediumCount(MediaCount { medium: Medium::Dvd, count: 3 }).apply(&mut held);
    assert_eq!(held, facts(&[(Medium::Cd, 1), (Medium::Dvd, 3)]));
}

/// A carrier the pressing lacks is added after the others, and removing one
/// it lacks changes nothing.
#[test]
fn a_medium_count_adds_a_missing_medium() {
    let mut held = facts(&[(Medium::Cd, 1)]);
    PressingFactEdit::MediumCount(MediaCount { medium: Medium::Dvd, count: 1 }).apply(&mut held);
    assert_eq!(held, facts(&[(Medium::Cd, 1), (Medium::Dvd, 1)]));
    PressingFactEdit::MediumCount(MediaCount { medium: Medium::Vinyl, count: 0 }).apply(&mut held);
    assert_eq!(held, facts(&[(Medium::Cd, 1), (Medium::Dvd, 1)]));
}

fn details(held: &[DiscogsDetail]) -> PressingFacts {
    PressingFacts { discogs_details: held.to_vec(), ..Default::default() }
}

/// An added detail goes after the others, and one the pressing holds keeps its
/// place.
#[test]
fn an_added_detail_joins_the_end_once() {
    let mut held = details(&[DiscogsDetail::Reissue]);
    PressingFactEdit::AddDiscogsDetail(DiscogsDetail::Stereo).apply(&mut held);
    PressingFactEdit::AddDiscogsDetail(DiscogsDetail::Reissue).apply(&mut held);
    assert_eq!(held, details(&[DiscogsDetail::Reissue, DiscogsDetail::Stereo]));
}

/// A removal takes only the named detail, and removing one the pressing lacks
/// changes nothing.
#[test]
fn a_removed_detail_leaves_the_others() {
    let mut held = details(&[DiscogsDetail::Reissue, DiscogsDetail::Stereo, DiscogsDetail::Mono]);
    PressingFactEdit::RemoveDiscogsDetail(DiscogsDetail::Stereo).apply(&mut held);
    PressingFactEdit::RemoveDiscogsDetail(DiscogsDetail::Flac).apply(&mut held);
    assert_eq!(held, details(&[DiscogsDetail::Reissue, DiscogsDetail::Mono]));
}
