use super::*;

fn japan_cd() -> BridgePressingFacts {
    BridgePressingFacts {
        area: Some(BridgeReleaseArea::Country { code: "JP".to_string() }),
        media: vec![BridgeMediaCount { medium: BridgeMedium::Cd, count: 1 }],
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
    }
}

/// Where a pressing was released is one part: a country by its code, a
/// region through the catalog, and nothing when the facts state none.
#[test]
fn the_area_is_one_part_a_country_or_a_region() {
    assert_eq!(
        bridge_area_terms(japan_cd()),
        vec![BridgeFactTerm::Country { code: "JP".to_string() }]
    );
    let europe = BridgePressingFacts {
        area: Some(BridgeReleaseArea::Region { region: BridgeRegion::UkAndEurope }),
        ..japan_cd()
    };
    assert_eq!(
        bridge_area_terms(europe),
        vec![BridgeFactTerm::Worded { label: localized("core.pressing.region.uk_and_europe") }]
    );
    assert_eq!(bridge_area_terms(BridgePressingFacts { area: None, ..japan_cd() }), Vec::new());
}

/// More than one of a medium carries its count.
#[test]
fn media_carry_their_count() {
    let two_vinyl = vec![BridgeMediaCount { medium: BridgeMedium::Vinyl, count: 2 }];
    assert_eq!(
        bridge_media_terms(two_vinyl),
        vec![BridgeFactTerm::Counted { count: 2, label: localized("core.pressing.medium.vinyl") }]
    );
    assert_eq!(
        bridge_media_terms(japan_cd().media),
        vec![BridgeFactTerm::Worded { label: verbatim("CD") }]
    );
}

/// The detail line names what sets a pressing apart; an ordinary
/// official release is not set apart by being official.
#[test]
fn details_leave_out_the_ordinary_official_status() {
    let promo = BridgePressingFacts {
        status: Some(BridgeReleaseStatus::Promotion),
        packaging: Some(BridgePackaging::Digipak),
        discogs_details: vec![BridgeDiscogsDetail::Reissue, BridgeDiscogsDetail::Flac],
        ..japan_cd()
    };
    assert_eq!(
        bridge_pressing_details(promo),
        vec![
            BridgeFactTerm::Worded { label: localized("core.pressing.status.promotion") },
            BridgeFactTerm::Worded { label: localized("core.pressing.packaging.digipak") },
            BridgeFactTerm::Worded { label: localized("core.pressing.discogs.reissue") },
            BridgeFactTerm::Worded { label: verbatim("FLAC") },
        ]
    );
    let official =
        BridgePressingFacts { status: Some(BridgeReleaseStatus::Official), ..japan_cd() };
    assert!(bridge_pressing_details(official).is_empty());
}

/// A country with a commonly used short form is worded through the catalog;
/// every other country keeps its code for the app to name in full.
#[test]
fn a_country_with_a_short_form_is_worded_through_the_catalog() {
    let area = |code: &str| BridgeReleaseArea::Country { code: code.to_string() };
    assert_eq!(
        bridge_area_term(area("US")),
        BridgeFactTerm::Worded { label: localized("core.pressing.country.us") }
    );
    assert_eq!(
        bridge_area_term(area("GB")),
        BridgeFactTerm::Worded { label: localized("core.pressing.country.uk") }
    );
    assert_eq!(bridge_area_term(area("JP")), BridgeFactTerm::Country { code: "JP".to_string() });
}
