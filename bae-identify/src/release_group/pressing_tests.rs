//! Which of a pressing's records leads its row, what picking the row claims,
//! and which catalogs the row names.

use super::*;
use crate::release_group::group_results;
use crate::release_group::tests::lead_ids;
use crate::release_group::tests::{agreed, discogs, grouped, mb, paired};
use bae_types::pressing::{Medium, StatedMedia};

/// A tracklist as a source states it. What it says does not matter here; that
/// it was said is the tie-break.
fn listed() -> bae_catalogs::search::SourceTracks {
    bae_catalogs::search::SourceTracks::Listed { count: 9 }
}

/// Both sources describe the disc and neither of them is the one the draft is
/// read from by name: the record the folder says more about leads the row, and
/// picking the row claims the other beside it.
#[test]
fn the_record_the_text_says_most_about_leads_its_pressing() {
    let (mb_release, dg_release) = paired();

    let groups = group_results(vec![(mb_release, agreed(1)), (dg_release, agreed(3))], None);

    assert_eq!(lead_ids(&groups[0]), vec![vec!["dg-1", "mb-1"]]);
    assert_eq!(
        groups[0].sections[0].pressings[0].pick(),
        crate::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                LookupCatalog::Discogs,
                "dg-1".to_string()
            ),
            partners: vec![bae_catalogs::records::LookupRef::new(
                LookupCatalog::MusicBrainz,
                "mb-1"
            )],
        }
    );
}

/// Records the folder says as much about: the one that states a tracklist
/// leads, since the draft's rows and the settle's check of them against the
/// audio are read out of that tracklist. A source that answered and listed
/// nothing states none.
#[test]
fn a_stated_tracklist_leads_records_the_text_says_as_much_about() {
    let (mut mb_release, mut dg_release) = paired();
    mb_release.source_tracks = Some(bae_catalogs::search::SourceTracks::Nothing);
    dg_release.source_tracks = Some(listed());

    let groups = group_results(vec![(mb_release, agreed(2)), (dg_release, agreed(2))], None);

    assert_eq!(lead_ids(&groups[0]), vec![vec!["dg-1", "mb-1"]]);
}

/// Nothing the folder says tells the two records apart and both state a
/// tracklist, so the source name has the last word.
#[test]
fn records_nothing_tells_apart_lead_with_musicbrainz() {
    let (mut mb_release, mut dg_release) = paired();
    mb_release.source_tracks = Some(listed());
    dg_release.source_tracks = Some(listed());

    let groups = group_results(vec![(dg_release, agreed(2)), (mb_release, agreed(2))], None);

    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
}

/// A row paired across both sources is picked whole: the lead is the
/// document the draft is read from, and the other source's record of the
/// same pressing rides along as a partner.
#[test]
fn a_paired_row_is_picked_with_its_partner() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["012345678905".to_string()];
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["012345678905".to_string()];

    let groups = grouped(vec![one, other]);
    assert_eq!(
        groups[0].sections[0].pressings[0].pick(),
        crate::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                LookupCatalog::MusicBrainz,
                "mb-1".to_string()
            ),
            partners: vec![bae_catalogs::records::LookupRef::new(LookupCatalog::Discogs, "dg-1")],
        }
    );
}

/// A pressing only one source lists claims only that source.
#[test]
fn a_lone_row_is_picked_with_no_partner() {
    let groups = grouped(vec![discogs("dg-1", Some("master-7"), Some(1992))]);
    assert_eq!(
        groups[0].sections[0].pressings[0].pick(),
        crate::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                LookupCatalog::Discogs,
                "dg-1".to_string()
            ),
            partners: vec![],
        }
    );
}

/// Which record a row is read from decides what the row shows, not the
/// order its catalogs are named in: a row led by its Discogs record still
/// names MusicBrainz first, and a row one catalog lists names that one alone.
#[test]
fn a_row_names_its_catalogs_in_the_order_surfaces_list_them() {
    let discogs_led = Pressing::over(vec![
        discogs("dg-1", Some("7"), Some(1992)),
        mb("mb-1", Some("g"), Some(1992)),
    ]);
    assert_eq!(discogs_led.sources(), [LookupCatalog::MusicBrainz, LookupCatalog::Discogs]);
    let unpaired = Pressing::over(vec![mb("mb-1", Some("g"), Some(1992))]);
    assert_eq!(unpaired.sources(), [LookupCatalog::MusicBrainz]);
}

/// A row of two records keeps each record's own facts: neither fills the
/// other, and what a pick applies is the lead's.
#[test]
fn a_pressings_records_keep_their_own_facts() {
    use bae_types::pressing::{area, DiscogsDetail, ReleaseStatus};
    let mut musicbrainz = mb("mb-1", Some("group-x"), Some(1992));
    musicbrainz.barcodes = vec!["012345678905".to_string()];
    musicbrainz.area = Some(area("JP"));
    musicbrainz.status = Some(ReleaseStatus::Official);
    musicbrainz.media = StatedMedia::PerMedium(vec![Some(Medium::Cd)]);
    let mut discogs = discogs("dg-1", Some("master-7"), Some(1992));
    discogs.barcodes = vec!["012345678905".to_string()];
    discogs.status = Some(ReleaseStatus::Promotion);
    discogs.discogs_details = vec![DiscogsDetail::Reissue];
    let groups = grouped(vec![musicbrainz, discogs]);
    let pressing = groups[0].sections[0].pressings[0].clone();
    assert_eq!(pressing.releases.len(), 2, "the two records are one pressing");
    let lead = pressing.lead().facts();
    assert_eq!(lead.area, Some(area("JP")));
    assert_eq!(lead.status, Some(ReleaseStatus::Official));
    assert!(lead.discogs_details.is_empty(), "Discogs's details stay on its record");
    let other = pressing.releases[1].facts();
    assert_eq!(other.status, Some(ReleaseStatus::Promotion));
    assert_eq!(other.discogs_details, vec![DiscogsDetail::Reissue]);
    assert_eq!(other.area, None, "MusicBrainz's area stays on its record");
}
