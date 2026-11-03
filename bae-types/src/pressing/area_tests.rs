use super::*;
use crate::pressing::recorded;

#[test]
fn the_discogs_table_is_discogss_own_list() {
    assert_eq!(
        DISCOGS_COUNTRIES.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        recorded(include_str!(
            "../../test-fixtures/pressing-vocabulary/discogs-release-countries.txt"
        ))
    );
    for (name, place) in DISCOGS_COUNTRIES {
        if let Place::Code(code) = place {
            assert!(Country::from_code(code).is_some(), "{name} names {code}");
        }
    }
}

/// Every code MusicBrainz states a release in names an area, and the
/// codes outside ISO 3166-1 are exactly the region codes.
#[test]
fn every_musicbrainz_code_names_an_area() {
    let page =
        include_str!("../../test-fixtures/pressing-vocabulary/musicbrainz-release-countries.txt");
    let mut outside_the_standard = Vec::new();
    for line in recorded(page) {
        let (code, name) = line.split_once('\t').expect("a code and a name");
        assert!(ReleaseArea::musicbrainz(code).is_some(), "{code} ({name})");
        if Country::from_code(code).is_none() {
            outside_the_standard.push(code);
        }
    }
    assert_eq!(
        outside_the_standard,
        MUSICBRAINZ_REGION_CODES.iter().map(|(code, _)| *code).collect::<Vec<_>>()
    );
}

/// Every region is one some catalog states a release in.
#[test]
fn every_region_is_named_by_a_catalog() {
    for region in Region::ALL {
        let named = MUSICBRAINZ_REGION_CODES.iter().any(|(_, r)| r == region)
            || DISCOGS_COUNTRIES
                .iter()
                .any(|(_, place)| matches!(place, Place::Region(r) if r == region));
        assert!(named, "{region:?}");
    }
}

/// Every region has one row of writings with a name, and capital
/// abbreviations.
#[test]
fn every_region_is_written_somehow() {
    for region in Region::ALL {
        let rows = REGION_WRITINGS.iter().filter(|(r, _, _)| r == region).count();
        assert_eq!(rows, 1, "{region:?}");
        assert!(!region.writings().0.is_empty(), "{region:?}");
        for code in region.writings().1 {
            assert!(code.chars().all(|c| c.is_ascii_uppercase()), "{code}");
        }
    }
}

#[test]
fn both_catalogs_name_the_same_areas_alike() {
    let japan = ReleaseArea::Country(Country::from_code("JP").unwrap());
    assert_eq!(ReleaseArea::musicbrainz("JP"), Some(japan));
    assert_eq!(ReleaseArea::discogs("Japan"), Some(japan));
    assert_eq!(
        ReleaseArea::discogs("UK"),
        Some(ReleaseArea::Country(Country::from_code("GB").unwrap()))
    );
    assert_eq!(ReleaseArea::musicbrainz("XE"), ReleaseArea::discogs("Europe"));
    assert_eq!(ReleaseArea::musicbrainz("SU"), ReleaseArea::discogs("USSR"));
    assert_eq!(ReleaseArea::discogs("uk & europe"), Some(ReleaseArea::Region(Region::UkAndEurope)));
    assert_eq!(ReleaseArea::discogs("Atlantis"), None);
    assert_eq!(ReleaseArea::musicbrainz("ZZ"), None);
}
