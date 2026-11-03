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

/// Both catalogs' rows read the same shape: the place, then the media.
#[test]
fn a_summary_is_the_place_then_the_media() {
    assert_eq!(
        bridge_pressing_summary(japan_cd()),
        vec![
            BridgeFactTerm::Country { code: "JP".to_string() },
            BridgeFactTerm::Worded { label: verbatim("CD") },
        ]
    );
    let two_vinyl_in_europe = BridgePressingFacts {
        area: Some(BridgeReleaseArea::Region { region: BridgeRegion::UkAndEurope }),
        media: vec![BridgeMediaCount { medium: BridgeMedium::Vinyl, count: 2 }],
        ..japan_cd()
    };
    assert_eq!(
        bridge_pressing_summary(two_vinyl_in_europe),
        vec![
            BridgeFactTerm::Worded { label: localized("core.pressing.region.uk_and_europe") },
            BridgeFactTerm::Counted { count: 2, label: localized("core.pressing.medium.vinyl") },
        ]
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
