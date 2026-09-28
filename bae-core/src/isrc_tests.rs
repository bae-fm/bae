use super::*;

fn country(code: &str) -> Option<ReleaseArea> {
    Country::from_code(code).map(ReleaseArea::Country)
}

/// A code's first two characters name the territory its agency serves, which
/// is not always the ISO code of that territory.
#[test]
fn an_agency_code_names_its_territory() {
    assert_eq!(territory("IT00G9170501"), country("IT"));
    assert_eq!(territory("it-00g-91-70501"), country("IT"));
    assert_eq!(territory("QM0000000001"), country("US"));
    assert_eq!(territory("FX0000000001"), country("FR"));
    assert_eq!(territory("UK0000000001"), country("GB"));
    assert_eq!(territory("BX0000000001"), country("BR"));
    assert_eq!(
        territory("YU0000000001"),
        Some(ReleaseArea::Region(Region::Yugoslavia))
    );
}

/// The worldwide codes, a code the list does not hold, and what is not an
/// ISRC at all name no territory.
#[test]
fn a_code_naming_no_territory_names_none() {
    for code in [
        "ZZ0000000001",
        "QN0000000001",
        "TC0000000001",
        "AQ0000000001",
        "IT00G917050",
        "IT00G91705X1",
        "1T00G9170501",
    ] {
        assert_eq!(territory(code), None, "{code}");
    }
}

/// The territory more than half of the codes naming one agree on; a split
/// names none.
#[test]
fn most_recordings_name_where_they_were_registered() {
    assert_eq!(
        registered_in([
            "IT0000000001",
            "IT0000000002",
            "DE0000000003",
            "ZZ0000000004"
        ]),
        country("IT")
    );
    assert_eq!(registered_in(["IT0000000001", "DE0000000002"]), None);
    assert_eq!(registered_in(["ZZ0000000001"]), None);
    assert_eq!(registered_in([]), None);
}
