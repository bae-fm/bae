use super::*;

use bae_catalogs::catalog::LookupCatalog;

fn listing(release_id: &str, count: u32) -> MetadataResult {
    MetadataResult {
        source_tracks: Some(SourceTracks::Listed { count }),
        ..MetadataResult::for_test(LookupCatalog::MusicBrainz, release_id, Some("g"))
    }
}

/// The pick is the pressing itself, lead first: what the settle applies
/// is what the rule judged.
#[test]
fn a_sole_fitting_pressing_is_picked_whole() {
    let records =
        vec![listing("mb-1", 11), MetadataResult::for_test(LookupCatalog::Discogs, "dg-1", None)];
    let pick = unattended_pick(&records, &[0, 0], None, 11).expect("it fits");
    assert_eq!(pick.records, records.as_slice());
    assert_eq!(pick.pressing().lead().release_id, "mb-1");
}

/// A document that went unread is named over a count nobody could
/// check.
#[test]
fn an_unread_document_is_named_over_an_unchecked_count() {
    let mut unread = MetadataResult::for_test(LookupCatalog::MusicBrainz, "mb-1", Some("g"));
    unread.document_failure = Some(bae_catalogs::LookupFailure::Network);
    assert_eq!(
        unattended_pick(std::slice::from_ref(&unread), &[0], None, 11),
        Err(Declined::UnreadDocument)
    );
}

/// A tracklist that could not be read is never a fit, whatever the
/// folder's count.
#[test]
fn an_unreadable_tracklist_is_never_a_fit() {
    assert_eq!(TracklistFit::of(Some(&SourceTracks::Unreadable), 2), TracklistFit::Unreadable);
    assert!(!TracklistFit::of(Some(&SourceTracks::Unreadable), 2).admits());
}

/// A row is ruled out only by a count its read tracklists disagree on,
/// with no record left that fits or is still to be read.
#[test]
fn a_row_is_ruled_out_only_by_a_read_count_that_disagrees() {
    let lists_nothing = MetadataResult {
        source_tracks: Some(SourceTracks::Nothing),
        ..MetadataResult::for_test(LookupCatalog::Discogs, "dg-1", None)
    };
    let unread = MetadataResult::for_test(LookupCatalog::Discogs, "dg-2", None);
    assert!(rules_out(&[listing("mb-1", 12)], 11));
    assert!(rules_out(&[listing("mb-1", 12), lists_nothing.clone()], 11));
    assert!(!rules_out(&[listing("mb-1", 12), listing("mb-2", 11)], 11));
    assert!(!rules_out(&[listing("mb-1", 12), unread.clone()], 11));
    assert!(!rules_out(&[lists_nothing], 11));
    assert!(!rules_out(&[unread], 11));
}
