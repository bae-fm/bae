//! The country a release's own field names, however it is written.
//!
//! A provider states where a pressing was made as a code — MusicBrainz answers
//! `JP` — and a folder writes it out — `Japan`. Neither spelling is more
//! correct than the other, so ranking asks this table for the rest of them: a
//! code's written-out names, or a name's code.
//!
//! Only the officially assigned ISO 3166-1 alpha-2 codes are here. A value
//! outside the standard — MusicBrainz answers `XE` for a Europe-wide release
//! and `XW` for a worldwide one — names no country, and is looked for in the
//! text as the plain string it is.

use std::collections::HashMap;
use std::sync::OnceLock;

/// One country: its alpha-2 code, and the English names it is written out as.
/// The first name is the standard's own; any after it are forms in common use
/// that a folder is as likely to print. Spacing and punctuation are not a
/// form of their own — `Viet Nam` is looked up the same as `Vietnam`.
pub(super) struct Country {
    pub code: &'static str,
    pub names: &'static [&'static str],
}

/// The country `value` names, whether `value` is its code or one of its names.
/// `None` when nothing in the standard is written that way.
///
/// Compared the way the candidate's text is read — case, spacing, punctuation
/// and diacritics dropped — so `JP`, `jp`, `Japan` and `japan` all reach the
/// same country.
pub(super) fn named(value: &str) -> Option<&'static Country> {
    let value = super::agreements::squash(value);
    (!value.is_empty())
        .then(|| index().get(&value).map(|&at| &COUNTRIES[at]))
        .flatten()
}

/// Every spelling in the table, to the country it names. Built once.
fn index() -> &'static HashMap<String, usize> {
    static INDEX: OnceLock<HashMap<String, usize>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::new();
        for (at, country) in COUNTRIES.iter().enumerate() {
            index.insert(super::agreements::squash(country.code), at);
            for name in country.names {
                index.insert(super::agreements::squash(name), at);
            }
        }
        index
    })
}

/// Shorthand for one row of the table.
macro_rules! country {
    ($code:literal, $($name:literal),+ $(,)?) => {
        Country {
            code: $code,
            names: &[$($name),+],
        }
    };
}

/// ISO 3166-1 alpha-2, in code order.
static COUNTRIES: &[Country] = &[
    country!("AD", "Andorra"),
    country!("AE", "United Arab Emirates", "UAE"),
    country!("AF", "Afghanistan"),
    country!("AG", "Antigua and Barbuda"),
    country!("AI", "Anguilla"),
    country!("AL", "Albania"),
    country!("AM", "Armenia"),
    country!("AO", "Angola"),
    country!("AQ", "Antarctica"),
    country!("AR", "Argentina"),
    country!("AS", "American Samoa"),
    country!("AT", "Austria"),
    country!("AU", "Australia"),
    country!("AW", "Aruba"),
    country!("AX", "Åland Islands"),
    country!("AZ", "Azerbaijan"),
    country!("BA", "Bosnia and Herzegovina"),
    country!("BB", "Barbados"),
    country!("BD", "Bangladesh"),
    country!("BE", "Belgium"),
    country!("BF", "Burkina Faso"),
    country!("BG", "Bulgaria"),
    country!("BH", "Bahrain"),
    country!("BI", "Burundi"),
    country!("BJ", "Benin"),
    country!("BL", "Saint Barthélemy"),
    country!("BM", "Bermuda"),
    country!("BN", "Brunei Darussalam", "Brunei"),
    country!("BO", "Bolivia"),
    country!(
        "BQ",
        "Bonaire, Sint Eustatius and Saba",
        "Caribbean Netherlands"
    ),
    country!("BR", "Brazil"),
    country!("BS", "Bahamas"),
    country!("BT", "Bhutan"),
    country!("BV", "Bouvet Island"),
    country!("BW", "Botswana"),
    country!("BY", "Belarus"),
    country!("BZ", "Belize"),
    country!("CA", "Canada"),
    country!("CC", "Cocos (Keeling) Islands"),
    country!("CD", "Democratic Republic of the Congo", "DR Congo"),
    country!("CF", "Central African Republic"),
    country!("CG", "Congo", "Republic of the Congo"),
    country!("CH", "Switzerland"),
    country!("CI", "Côte d'Ivoire", "Ivory Coast"),
    country!("CK", "Cook Islands"),
    country!("CL", "Chile"),
    country!("CM", "Cameroon"),
    country!("CN", "China"),
    country!("CO", "Colombia"),
    country!("CR", "Costa Rica"),
    country!("CU", "Cuba"),
    country!("CV", "Cabo Verde", "Cape Verde"),
    country!("CW", "Curaçao"),
    country!("CX", "Christmas Island"),
    country!("CY", "Cyprus"),
    country!("CZ", "Czechia", "Czech Republic"),
    country!("DE", "Germany"),
    country!("DJ", "Djibouti"),
    country!("DK", "Denmark"),
    country!("DM", "Dominica"),
    country!("DO", "Dominican Republic"),
    country!("DZ", "Algeria"),
    country!("EC", "Ecuador"),
    country!("EE", "Estonia"),
    country!("EG", "Egypt"),
    country!("EH", "Western Sahara"),
    country!("ER", "Eritrea"),
    country!("ES", "Spain"),
    country!("ET", "Ethiopia"),
    country!("FI", "Finland"),
    country!("FJ", "Fiji"),
    country!("FK", "Falkland Islands"),
    country!("FM", "Micronesia"),
    country!("FO", "Faroe Islands"),
    country!("FR", "France"),
    country!("GA", "Gabon"),
    country!("GB", "United Kingdom", "UK", "Great Britain"),
    country!("GD", "Grenada"),
    country!("GE", "Georgia"),
    country!("GF", "French Guiana"),
    country!("GG", "Guernsey"),
    country!("GH", "Ghana"),
    country!("GI", "Gibraltar"),
    country!("GL", "Greenland"),
    country!("GM", "Gambia"),
    country!("GN", "Guinea"),
    country!("GP", "Guadeloupe"),
    country!("GQ", "Equatorial Guinea"),
    country!("GR", "Greece"),
    country!("GS", "South Georgia and the South Sandwich Islands"),
    country!("GT", "Guatemala"),
    country!("GU", "Guam"),
    country!("GW", "Guinea-Bissau"),
    country!("GY", "Guyana"),
    country!("HK", "Hong Kong"),
    country!("HM", "Heard Island and McDonald Islands"),
    country!("HN", "Honduras"),
    country!("HR", "Croatia"),
    country!("HT", "Haiti"),
    country!("HU", "Hungary"),
    country!("ID", "Indonesia"),
    country!("IE", "Ireland"),
    country!("IL", "Israel"),
    country!("IM", "Isle of Man"),
    country!("IN", "India"),
    country!("IO", "British Indian Ocean Territory"),
    country!("IQ", "Iraq"),
    country!("IR", "Iran"),
    country!("IS", "Iceland"),
    country!("IT", "Italy"),
    country!("JE", "Jersey"),
    country!("JM", "Jamaica"),
    country!("JO", "Jordan"),
    country!("JP", "Japan"),
    country!("KE", "Kenya"),
    country!("KG", "Kyrgyzstan"),
    country!("KH", "Cambodia"),
    country!("KI", "Kiribati"),
    country!("KM", "Comoros"),
    country!("KN", "Saint Kitts and Nevis"),
    country!("KP", "North Korea"),
    country!("KR", "South Korea"),
    country!("KW", "Kuwait"),
    country!("KY", "Cayman Islands"),
    country!("KZ", "Kazakhstan"),
    country!("LA", "Laos"),
    country!("LB", "Lebanon"),
    country!("LC", "Saint Lucia"),
    country!("LI", "Liechtenstein"),
    country!("LK", "Sri Lanka"),
    country!("LR", "Liberia"),
    country!("LS", "Lesotho"),
    country!("LT", "Lithuania"),
    country!("LU", "Luxembourg"),
    country!("LV", "Latvia"),
    country!("LY", "Libya"),
    country!("MA", "Morocco"),
    country!("MC", "Monaco"),
    country!("MD", "Moldova"),
    country!("ME", "Montenegro"),
    country!("MF", "Saint Martin"),
    country!("MG", "Madagascar"),
    country!("MH", "Marshall Islands"),
    country!("MK", "North Macedonia", "Macedonia"),
    country!("ML", "Mali"),
    country!("MM", "Myanmar", "Burma"),
    country!("MN", "Mongolia"),
    country!("MO", "Macao", "Macau"),
    country!("MP", "Northern Mariana Islands"),
    country!("MQ", "Martinique"),
    country!("MR", "Mauritania"),
    country!("MS", "Montserrat"),
    country!("MT", "Malta"),
    country!("MU", "Mauritius"),
    country!("MV", "Maldives"),
    country!("MW", "Malawi"),
    country!("MX", "Mexico"),
    country!("MY", "Malaysia"),
    country!("MZ", "Mozambique"),
    country!("NA", "Namibia"),
    country!("NC", "New Caledonia"),
    country!("NE", "Niger"),
    country!("NF", "Norfolk Island"),
    country!("NG", "Nigeria"),
    country!("NI", "Nicaragua"),
    country!("NL", "Netherlands"),
    country!("NO", "Norway"),
    country!("NP", "Nepal"),
    country!("NR", "Nauru"),
    country!("NU", "Niue"),
    country!("NZ", "New Zealand"),
    country!("OM", "Oman"),
    country!("PA", "Panama"),
    country!("PE", "Peru"),
    country!("PF", "French Polynesia"),
    country!("PG", "Papua New Guinea"),
    country!("PH", "Philippines"),
    country!("PK", "Pakistan"),
    country!("PL", "Poland"),
    country!("PM", "Saint Pierre and Miquelon"),
    country!("PN", "Pitcairn"),
    country!("PR", "Puerto Rico"),
    country!("PS", "Palestine"),
    country!("PT", "Portugal"),
    country!("PW", "Palau"),
    country!("PY", "Paraguay"),
    country!("QA", "Qatar"),
    country!("RE", "Réunion"),
    country!("RO", "Romania"),
    country!("RS", "Serbia"),
    country!("RU", "Russia", "Russian Federation"),
    country!("RW", "Rwanda"),
    country!("SA", "Saudi Arabia"),
    country!("SB", "Solomon Islands"),
    country!("SC", "Seychelles"),
    country!("SD", "Sudan"),
    country!("SE", "Sweden"),
    country!("SG", "Singapore"),
    country!("SH", "Saint Helena"),
    country!("SI", "Slovenia"),
    country!("SJ", "Svalbard and Jan Mayen"),
    country!("SK", "Slovakia"),
    country!("SL", "Sierra Leone"),
    country!("SM", "San Marino"),
    country!("SN", "Senegal"),
    country!("SO", "Somalia"),
    country!("SR", "Suriname"),
    country!("SS", "South Sudan"),
    country!("ST", "Sao Tome and Principe"),
    country!("SV", "El Salvador"),
    country!("SX", "Sint Maarten"),
    country!("SY", "Syria", "Syrian Arab Republic"),
    country!("SZ", "Eswatini", "Swaziland"),
    country!("TC", "Turks and Caicos Islands"),
    country!("TD", "Chad"),
    country!("TF", "French Southern Territories"),
    country!("TG", "Togo"),
    country!("TH", "Thailand"),
    country!("TJ", "Tajikistan"),
    country!("TK", "Tokelau"),
    country!("TL", "Timor-Leste", "East Timor"),
    country!("TM", "Turkmenistan"),
    country!("TN", "Tunisia"),
    country!("TO", "Tonga"),
    country!("TR", "Türkiye", "Turkey"),
    country!("TT", "Trinidad and Tobago"),
    country!("TV", "Tuvalu"),
    country!("TW", "Taiwan"),
    country!("TZ", "Tanzania"),
    country!("UA", "Ukraine"),
    country!("UG", "Uganda"),
    country!("UM", "United States Minor Outlying Islands"),
    country!("US", "United States", "USA", "United States of America"),
    country!("UY", "Uruguay"),
    country!("UZ", "Uzbekistan"),
    country!("VA", "Holy See", "Vatican City"),
    country!("VC", "Saint Vincent and the Grenadines"),
    country!("VE", "Venezuela"),
    country!("VG", "British Virgin Islands"),
    country!("VI", "United States Virgin Islands"),
    country!("VN", "Viet Nam"),
    country!("VU", "Vanuatu"),
    country!("WF", "Wallis and Futuna"),
    country!("WS", "Samoa"),
    country!("YE", "Yemen"),
    country!("YT", "Mayotte"),
    country!("ZA", "South Africa"),
    country!("ZM", "Zambia"),
    country!("ZW", "Zimbabwe"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every spelling reaches exactly one country: a code or a name that two
    /// rows share would silently answer for whichever came last.
    #[test]
    fn no_spelling_names_two_countries() {
        let mut seen: HashMap<String, &str> = HashMap::new();
        for country in COUNTRIES {
            for spelling in std::iter::once(country.code).chain(country.names.iter().copied()) {
                let key = super::super::agreements::squash(spelling);
                assert!(!key.is_empty(), "{spelling} squashes to nothing");
                if let Some(other) = seen.insert(key, country.code) {
                    panic!("{spelling} names both {other} and {}", country.code);
                }
            }
        }
    }

    #[test]
    fn every_code_is_two_letters_in_order() {
        let codes: Vec<&str> = COUNTRIES.iter().map(|country| country.code).collect();
        assert!(codes
            .iter()
            .all(|code| code.len() == 2 && code.chars().all(|c| c.is_ascii_uppercase())));
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        assert_eq!(codes, sorted, "the table is in code order");
    }

    #[test]
    fn a_code_and_its_names_reach_the_same_country() {
        let japan = named("JP").expect("JP is a country");
        assert_eq!(japan.code, "JP");
        assert_eq!(named("japan").map(|c| c.code), Some("JP"));
        assert_eq!(named("U.S.A.").map(|c| c.code), Some("US"));
        assert_eq!(named("côte d'ivoire").map(|c| c.code), Some("CI"));
    }

    /// A value the standard does not carry names no country — MusicBrainz's
    /// `XE` and `XW` among them.
    #[test]
    fn a_value_outside_the_standard_names_no_country() {
        assert!(named("XE").is_none());
        assert!(named("XW").is_none());
        assert!(named("").is_none());
        assert!(named("Atlantic").is_none());
    }
}
