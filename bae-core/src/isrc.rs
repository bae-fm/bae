//! ISRC codes — the code a recording is registered under, which a file's tags
//! may carry — and the territory each one's first two characters name.
//!
//! An ISRC is twelve characters: a two-letter agency code, a three-character
//! registrant code, two digits of year and a five-digit designation. The
//! agency code is not always the ISO 3166-1 code of its territory: the table
//! below is the International ISRC Agency's list of agency codes ("ISRC Agency
//! Code Allocations", November 2025). Brazil holds `BC`, `BK`, `BP` and `BX`
//! besides `BR`; the United States `QM`, `QT` and `QZ` besides `US`; the
//! United Kingdom `GX` and `UK`; France `FX`; Canada `CB`; South Korea `KS`;
//! South Africa `ZB`; Denmark's agency `FO` and `GL`. The worldwide codes
//! `CP`, `DG`, `QN`, `VV` and `ZZ` and TuneCore's `TC` name no territory,
//! and a code the list does not hold names none.

use crate::pressing::{Country, Region, ReleaseArea};

/// The territory the agency code of `isrc` was allocated for, when it is a
/// well-formed ISRC whose agency code names one. Hyphens and spaces a tag
/// writes between its parts are read through, and case is not.
pub fn territory(isrc: &str) -> Option<ReleaseArea> {
    let code: String = isrc
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let well_formed = code.len() == 12
        && code[..2].chars().all(|c| c.is_ascii_uppercase())
        && code[2..5].chars().all(|c| c.is_ascii_alphanumeric())
        && code[5..].chars().all(|c| c.is_ascii_digit());
    if !well_formed {
        return None;
    }
    AGENCY_CODES
        .iter()
        .find(|(agency, _)| *agency == &code[..2])
        .map(|(_, territory)| match territory {
            Territory::Country(country) => ReleaseArea::Country(
                Country::from_code(country).expect("the agency table names ISO 3166-1 codes"),
            ),
            Territory::Region(region) => ReleaseArea::Region(*region),
        })
}

/// Where most of these recordings were registered: the territory more than
/// half of the codes that name one agree on. `None` when no territory is named
/// by more than half of them, or none is named at all.
pub fn registered_in<'a>(isrcs: impl IntoIterator<Item = &'a str>) -> Option<ReleaseArea> {
    let named: Vec<ReleaseArea> = isrcs.into_iter().filter_map(territory).collect();
    let mut counts: Vec<(ReleaseArea, usize)> = Vec::new();
    for area in &named {
        match counts.iter_mut().find(|(counted, _)| counted == area) {
            Some((_, count)) => *count += 1,
            None => counts.push((*area, 1)),
        }
    }
    counts
        .into_iter()
        .find(|(_, count)| count * 2 > named.len())
        .map(|(area, _)| area)
}

/// The territory an agency code was allocated for.
enum Territory {
    /// A country, by its ISO 3166-1 code.
    Country(&'static str),
    Region(Region),
}

/// Every agency code that names a territory, with the territory.
const AGENCY_CODES: &[(&str, Territory)] = &[
    ("AD", Territory::Country("AD")),
    ("AE", Territory::Country("AE")),
    ("AF", Territory::Country("AF")),
    ("AG", Territory::Country("AG")),
    ("AI", Territory::Country("AI")),
    ("AL", Territory::Country("AL")),
    ("AM", Territory::Country("AM")),
    ("AO", Territory::Country("AO")),
    ("AR", Territory::Country("AR")),
    ("AT", Territory::Country("AT")),
    ("AU", Territory::Country("AU")),
    ("AW", Territory::Country("AW")),
    ("AZ", Territory::Country("AZ")),
    ("BA", Territory::Country("BA")),
    ("BB", Territory::Country("BB")),
    ("BC", Territory::Country("BR")),
    ("BD", Territory::Country("BD")),
    ("BE", Territory::Country("BE")),
    ("BF", Territory::Country("BF")),
    ("BG", Territory::Country("BG")),
    ("BH", Territory::Country("BH")),
    ("BI", Territory::Country("BI")),
    ("BJ", Territory::Country("BJ")),
    ("BK", Territory::Country("BR")),
    ("BM", Territory::Country("BM")),
    ("BN", Territory::Country("BN")),
    ("BO", Territory::Country("BO")),
    ("BP", Territory::Country("BR")),
    ("BR", Territory::Country("BR")),
    ("BS", Territory::Country("BS")),
    ("BT", Territory::Country("BT")),
    ("BW", Territory::Country("BW")),
    ("BX", Territory::Country("BR")),
    ("BY", Territory::Country("BY")),
    ("BZ", Territory::Country("BZ")),
    ("CA", Territory::Country("CA")),
    ("CB", Territory::Country("CA")),
    ("CD", Territory::Country("CD")),
    ("CF", Territory::Country("CF")),
    ("CG", Territory::Country("CG")),
    ("CH", Territory::Country("CH")),
    ("CI", Territory::Country("CI")),
    ("CL", Territory::Country("CL")),
    ("CM", Territory::Country("CM")),
    ("CN", Territory::Country("CN")),
    ("CO", Territory::Country("CO")),
    ("CR", Territory::Country("CR")),
    ("CS", Territory::Region(Region::SerbiaAndMontenegro)),
    ("CU", Territory::Country("CU")),
    ("CV", Territory::Country("CV")),
    ("CW", Territory::Country("CW")),
    ("CY", Territory::Country("CY")),
    ("CZ", Territory::Country("CZ")),
    ("DE", Territory::Country("DE")),
    ("DK", Territory::Country("DK")),
    ("DM", Territory::Country("DM")),
    ("DO", Territory::Country("DO")),
    ("DZ", Territory::Country("DZ")),
    ("EC", Territory::Country("EC")),
    ("EE", Territory::Country("EE")),
    ("EG", Territory::Country("EG")),
    ("ES", Territory::Country("ES")),
    ("ET", Territory::Country("ET")),
    ("FI", Territory::Country("FI")),
    ("FJ", Territory::Country("FJ")),
    ("FO", Territory::Country("DK")),
    ("FR", Territory::Country("FR")),
    ("FX", Territory::Country("FR")),
    ("GA", Territory::Country("GA")),
    ("GB", Territory::Country("GB")),
    ("GD", Territory::Country("GD")),
    ("GE", Territory::Country("GE")),
    ("GG", Territory::Country("GG")),
    ("GH", Territory::Country("GH")),
    ("GI", Territory::Country("GI")),
    ("GL", Territory::Country("DK")),
    ("GM", Territory::Country("GM")),
    ("GN", Territory::Country("GN")),
    ("GQ", Territory::Country("GQ")),
    ("GR", Territory::Country("GR")),
    ("GT", Territory::Country("GT")),
    ("GW", Territory::Country("GW")),
    ("GX", Territory::Country("GB")),
    ("GY", Territory::Country("GY")),
    ("HK", Territory::Country("HK")),
    ("HN", Territory::Country("HN")),
    ("HR", Territory::Country("HR")),
    ("HT", Territory::Country("HT")),
    ("HU", Territory::Country("HU")),
    ("ID", Territory::Country("ID")),
    ("IE", Territory::Country("IE")),
    ("IL", Territory::Country("IL")),
    ("IM", Territory::Country("IM")),
    ("IN", Territory::Country("IN")),
    ("IQ", Territory::Country("IQ")),
    ("IR", Territory::Country("IR")),
    ("IS", Territory::Country("IS")),
    ("IT", Territory::Country("IT")),
    ("JE", Territory::Country("JE")),
    ("JM", Territory::Country("JM")),
    ("JO", Territory::Country("JO")),
    ("JP", Territory::Country("JP")),
    ("KE", Territory::Country("KE")),
    ("KG", Territory::Country("KG")),
    ("KH", Territory::Country("KH")),
    ("KM", Territory::Country("KM")),
    ("KN", Territory::Country("KN")),
    ("KR", Territory::Country("KR")),
    ("KS", Territory::Country("KR")),
    ("KW", Territory::Country("KW")),
    ("KY", Territory::Country("KY")),
    ("KZ", Territory::Country("KZ")),
    ("LA", Territory::Country("LA")),
    ("LB", Territory::Country("LB")),
    ("LC", Territory::Country("LC")),
    ("LI", Territory::Country("LI")),
    ("LK", Territory::Country("LK")),
    ("LR", Territory::Country("LR")),
    ("LS", Territory::Country("LS")),
    ("LT", Territory::Country("LT")),
    ("LU", Territory::Country("LU")),
    ("LV", Territory::Country("LV")),
    ("MA", Territory::Country("MA")),
    ("MC", Territory::Country("MC")),
    ("MD", Territory::Country("MD")),
    ("ME", Territory::Country("ME")),
    ("MF", Territory::Country("MF")),
    ("MG", Territory::Country("MG")),
    ("MK", Territory::Country("MK")),
    ("ML", Territory::Country("ML")),
    ("MM", Territory::Country("MM")),
    ("MN", Territory::Country("MN")),
    ("MO", Territory::Country("MO")),
    ("MP", Territory::Country("MP")),
    ("MR", Territory::Country("MR")),
    ("MS", Territory::Country("MS")),
    ("MT", Territory::Country("MT")),
    ("MU", Territory::Country("MU")),
    ("MV", Territory::Country("MV")),
    ("MW", Territory::Country("MW")),
    ("MX", Territory::Country("MX")),
    ("MY", Territory::Country("MY")),
    ("MZ", Territory::Country("MZ")),
    ("NA", Territory::Country("NA")),
    ("NE", Territory::Country("NE")),
    ("NG", Territory::Country("NG")),
    ("NI", Territory::Country("NI")),
    ("NL", Territory::Country("NL")),
    ("NO", Territory::Country("NO")),
    ("NP", Territory::Country("NP")),
    ("NZ", Territory::Country("NZ")),
    ("OM", Territory::Country("OM")),
    ("PA", Territory::Country("PA")),
    ("PE", Territory::Country("PE")),
    ("PF", Territory::Country("PF")),
    ("PG", Territory::Country("PG")),
    ("PH", Territory::Country("PH")),
    ("PK", Territory::Country("PK")),
    ("PL", Territory::Country("PL")),
    ("PR", Territory::Country("PR")),
    ("PS", Territory::Country("PS")),
    ("PT", Territory::Country("PT")),
    ("PY", Territory::Country("PY")),
    ("QA", Territory::Country("QA")),
    ("QM", Territory::Country("US")),
    ("QT", Territory::Country("US")),
    ("QZ", Territory::Country("US")),
    ("RO", Territory::Country("RO")),
    ("RS", Territory::Country("RS")),
    ("RU", Territory::Country("RU")),
    ("RW", Territory::Country("RW")),
    ("SA", Territory::Country("SA")),
    ("SB", Territory::Country("SB")),
    ("SC", Territory::Country("SC")),
    ("SD", Territory::Country("SD")),
    ("SE", Territory::Country("SE")),
    ("SG", Territory::Country("SG")),
    ("SI", Territory::Country("SI")),
    ("SK", Territory::Country("SK")),
    ("SL", Territory::Country("SL")),
    ("SM", Territory::Country("SM")),
    ("SN", Territory::Country("SN")),
    ("SO", Territory::Country("SO")),
    ("SR", Territory::Country("SR")),
    ("SS", Territory::Country("SS")),
    ("SV", Territory::Country("SV")),
    ("SX", Territory::Country("SX")),
    ("SY", Territory::Country("SY")),
    ("SZ", Territory::Country("SZ")),
    ("TD", Territory::Country("TD")),
    ("TG", Territory::Country("TG")),
    ("TH", Territory::Country("TH")),
    ("TL", Territory::Country("TL")),
    ("TN", Territory::Country("TN")),
    ("TO", Territory::Country("TO")),
    ("TR", Territory::Country("TR")),
    ("TT", Territory::Country("TT")),
    ("TW", Territory::Country("TW")),
    ("TZ", Territory::Country("TZ")),
    ("UA", Territory::Country("UA")),
    ("UG", Territory::Country("UG")),
    ("UK", Territory::Country("GB")),
    ("US", Territory::Country("US")),
    ("UY", Territory::Country("UY")),
    ("UZ", Territory::Country("UZ")),
    ("VC", Territory::Country("VC")),
    ("VE", Territory::Country("VE")),
    ("VG", Territory::Country("VG")),
    ("VN", Territory::Country("VN")),
    ("VU", Territory::Country("VU")),
    ("XK", Territory::Region(Region::Kosovo)),
    ("YE", Territory::Country("YE")),
    ("YU", Territory::Region(Region::Yugoslavia)),
    ("ZA", Territory::Country("ZA")),
    ("ZB", Territory::Country("ZA")),
    ("ZM", Territory::Country("ZM")),
    ("ZW", Territory::Country("ZW")),
];

#[cfg(test)]
#[path = "isrc_tests.rs"]
mod tests;
