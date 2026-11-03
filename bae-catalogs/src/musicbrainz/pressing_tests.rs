use super::*;
use bae_types::pressing::{Country, MediaCount, Region};

/// A release modelled on a real Japanese CD issue: an ISO code, the
/// Official status, a jewel case, one CD.
#[test]
fn a_release_reads_as_typed_facts() {
    let (facts, media) = read(Stated {
        release_id: "r",
        country: Some("JP"),
        status: Some("Official"),
        packaging: Some("Jewel Case"),
        media: [Some("CD")],
    });
    assert_eq!(
        facts,
        PressingFacts {
            area: Some(ReleaseArea::Country(Country::from_code("JP").unwrap())),
            media: vec![MediaCount { medium: Medium::Cd, count: 1 }],
            status: Some(ReleaseStatus::Official),
            packaging: Some(Packaging::JewelCase),
            discogs_details: Vec::new(),
        }
    );
    assert_eq!(media, StatedMedia::PerMedium(vec![Some(Medium::Cd)]));
}

#[test]
fn every_medium_is_counted_not_only_the_first() {
    let (facts, _) = read(Stated {
        release_id: "r",
        country: Some("XE"),
        status: None,
        packaging: None,
        media: [Some("CD"), Some("CD"), Some("DVD-Video"), None],
    });
    assert_eq!(
        facts.media,
        vec![
            MediaCount { medium: Medium::Cd, count: 2 },
            MediaCount { medium: Medium::Dvd, count: 1 },
        ]
    );
    assert_eq!(facts.area, Some(ReleaseArea::Region(Region::Europe)));
}

#[test]
fn a_name_outside_the_lists_is_left_out() {
    let (facts, media) = read(Stated {
        release_id: "r",
        country: Some("ZZ"),
        status: Some("Leaked"),
        packaging: Some("Crate"),
        media: [Some("Wax Tablet")],
    });
    assert_eq!(facts, PressingFacts::default());
    assert_eq!(media, StatedMedia::PerMedium(vec![None]));
}
