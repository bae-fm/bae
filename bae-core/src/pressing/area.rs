//! Where a pressing was released: a country, or a region no one country
//! covers.
//!
//! MusicBrainz states a release's country as a code: an ISO 3166-1 code, or
//! one of its own for an area the standard lacks — `XE` for Europe, `XW` for
//! the world, `SU` for the Soviet Union. Discogs writes a name: a country's
//! ("Japan", "UK"), a market's ("UK & Europe"), a former state's ("USSR").
//! Both are read into a [`ReleaseArea`]: a [`Country`] a surface names in the
//! reader's language, or a [`Region`] from a closed list, one variant per area
//! either catalog names that is not a current country.

use super::Country;

/// Where a pressing was released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ReleaseArea {
    Country(Country),
    Region(Region),
}

super::vocabulary! {
    /// An area a release is issued in that no current ISO 3166-1 code names:
    /// a market spanning several countries, a state that no longer exists, or
    /// a territory the standard does not list.
    pub enum Region {
        // Markets spanning more than one country.
        /// Europe as one market.
        Europe => "europe",
        /// Every market at once.
        Worldwide => "worldwide",
        Africa => "africa",
        Asia => "asia",
        MiddleEast => "middle_east",
        SouthEastAsia => "south_east_asia",
        CentralAmerica => "central_america",
        SouthAmerica => "south_america",
        /// North America with Mexico.
        NorthAmerica => "north_america",
        NorthAndSouthAmerica => "north_and_south_america",
        Australasia => "australasia",
        SouthPacific => "south_pacific",
        Scandinavia => "scandinavia",
        /// Belgium, the Netherlands and Luxembourg.
        Benelux => "benelux",
        /// The states of the Gulf Cooperation Council.
        GulfCooperationCouncil => "gulf_cooperation_council",
        UkAndEurope => "uk_and_europe",
        UkAndIreland => "uk_and_ireland",
        UkAndUs => "uk_and_us",
        UkAndFrance => "uk_and_france",
        UkAndGermany => "uk_and_germany",
        UkEuropeAndUs => "uk_europe_and_us",
        UkEuropeAndJapan => "uk_europe_and_japan",
        UkEuropeAndIsrael => "uk_europe_and_israel",
        UsaAndCanada => "usa_and_canada",
        UsaAndEurope => "usa_and_europe",
        UsaCanadaAndEurope => "usa_canada_and_europe",
        UsaCanadaAndUk => "usa_canada_and_uk",
        GermanyAndSwitzerland => "germany_and_switzerland",
        GermanyAustriaAndSwitzerland => "germany_austria_and_switzerland",
        FranceAndBenelux => "france_and_benelux",
        CzechRepublicAndSlovakia => "czech_republic_and_slovakia",
        /// Russia and the Commonwealth of Independent States.
        RussiaAndCis => "russia_and_cis",
        AustraliaAndNewZealand => "australia_and_new_zealand",
        SingaporeAndMalaysia => "singapore_and_malaysia",
        SingaporeMalaysiaAndHongKong => "singapore_malaysia_and_hong_kong",
        SingaporeMalaysiaHongKongAndThailand => "singapore_malaysia_hong_kong_and_thailand",
        HongKongAndThailand => "hong_kong_and_thailand",
        // States that no longer exist, and territories ISO 3166-1 does not
        // list.
        SovietUnion => "soviet_union",
        Yugoslavia => "yugoslavia",
        Czechoslovakia => "czechoslovakia",
        /// The German Democratic Republic.
        EastGermany => "east_germany",
        SerbiaAndMontenegro => "serbia_and_montenegro",
        NetherlandsAntilles => "netherlands_antilles",
        /// Kosovo, which ISO 3166-1 does not list.
        Kosovo => "kosovo",
        /// Abkhazia, which ISO 3166-1 does not list.
        Abkhazia => "abkhazia",
        AustriaHungary => "austria_hungary",
        OttomanEmpire => "ottoman_empire",
        Bohemia => "bohemia",
        ProtectorateOfBohemiaAndMoravia => "protectorate_of_bohemia_and_moravia",
        /// Korea before its division in 1945.
        KoreaBefore1945 => "korea_before1945",
        SouthVietnam => "south_vietnam",
        /// French Indochina.
        Indochina => "indochina",
        DutchEastIndies => "dutch_east_indies",
        BelgianCongo => "belgian_congo",
        Zaire => "zaire",
        Rhodesia => "rhodesia",
        SouthernRhodesia => "southern_rhodesia",
        SouthWestAfrica => "south_west_africa",
        Dahomey => "dahomey",
        UpperVolta => "upper_volta",
        Zanzibar => "zanzibar",
        ItalianEastAfrica => "italian_east_africa",
    }
}

desktop_only! {
    impl ReleaseArea {
        /// The area a MusicBrainz release's `country` code names. `None` for a
        /// code outside MusicBrainz's list, which the caller logs.
        pub(crate) fn musicbrainz(code: &str) -> Option<Self> {
            Country::from_code(code).map(Self::Country).or_else(|| {
                MUSICBRAINZ_REGION_CODES
                    .iter()
                    .find(|(stated, _)| stated.eq_ignore_ascii_case(code))
                    .map(|(_, region)| Self::Region(*region))
            })
        }

        /// The area a Discogs release's `country` names. `None` for a name
        /// outside Discogs's list, which the caller logs.
        pub(crate) fn discogs(name: &str) -> Option<Self> {
            DISCOGS_COUNTRIES
                .iter()
                .find(|(stated, _)| super::same_name(stated, name))
                .map(|(_, place)| place.area())
        }
    }

    impl Region {
        /// Every name a catalog writes this region as: its MusicBrainz code,
        /// and the country names Discogs gives it.
        pub(crate) fn written_names(self) -> impl Iterator<Item = &'static str> {
            MUSICBRAINZ_REGION_CODES
                .iter()
                .filter(move |(_, region)| *region == self)
                .map(|(code, _)| *code)
                .chain(
                    DISCOGS_COUNTRIES
                        .iter()
                        .filter(move |(_, place)| matches!(place, Place::Region(r) if *r == self))
                        .map(|(name, _)| *name),
                )
        }
    }

    /// The codes MusicBrainz gives the areas it states releases in that ISO
    /// 3166-1 does not assign, from the same page as the rest of its codes:
    /// <https://musicbrainz.org/statistics/countries>, captured 2026-09-25.
    const MUSICBRAINZ_REGION_CODES: &[(&str, Region)] = &[
        ("AN", Region::NetherlandsAntilles),
        ("CS", Region::SerbiaAndMontenegro),
        ("SU", Region::SovietUnion),
        ("XC", Region::Czechoslovakia),
        ("XE", Region::Europe),
        ("XG", Region::EastGermany),
        ("XK", Region::Kosovo),
        ("XW", Region::Worldwide),
        ("YU", Region::Yugoslavia),
    ];

    /// What one Discogs country name names: a country, by its code, or a
    /// region.
    #[derive(Debug, Clone, Copy)]
    enum Place {
        Code(&'static str),
        Region(Region),
    }

    impl Place {
        fn area(self) -> ReleaseArea {
            match self {
                Self::Code(code) => ReleaseArea::Country(
                    Country::from_code(code).expect("the Discogs table names assigned codes"),
                ),
                Self::Region(region) => ReleaseArea::Region(region),
            }
        }
    }

    /// Every country name Discogs states a release in, and the area it
    /// names. Source: every distinct `<country>` value of the Discogs
    /// releases data dump `discogs_20260901_releases.xml.gz`
    /// (<https://data.discogs.com/?prefix=data/2026/>), read 2026-09-25; the
    /// download ended after 16,930,842 of its releases. Ordered
    /// case-insensitively.
    ///
    /// A name for a country that no longer exists but whose territory one
    /// current country is — "Zaire", "Upper Volta" — is kept as its own
    /// region: the record states the name the sleeve prints, and a current
    /// country's name would be a different fact.
    const DISCOGS_COUNTRIES: &[(&str, Place)] = &[
        ("Abkhazia", Place::Region(Region::Abkhazia)),
        ("Afghanistan", Place::Code("AF")),
        ("Africa", Place::Region(Region::Africa)),
        ("Albania", Place::Code("AL")),
        ("Algeria", Place::Code("DZ")),
        ("American Samoa", Place::Code("AS")),
        ("Andorra", Place::Code("AD")),
        ("Angola", Place::Code("AO")),
        ("Anguilla", Place::Code("AI")),
        ("Antigua & Barbuda", Place::Code("AG")),
        ("Argentina", Place::Code("AR")),
        ("Armenia", Place::Code("AM")),
        ("Aruba", Place::Code("AW")),
        ("Asia", Place::Region(Region::Asia)),
        ("Australasia", Place::Region(Region::Australasia)),
        ("Australia", Place::Code("AU")),
        ("Australia & New Zealand", Place::Region(Region::AustraliaAndNewZealand)),
        ("Austria", Place::Code("AT")),
        ("Austria-Hungary", Place::Region(Region::AustriaHungary)),
        ("Azerbaijan", Place::Code("AZ")),
        ("Bahamas, The", Place::Code("BS")),
        ("Bahrain", Place::Code("BH")),
        ("Bangladesh", Place::Code("BD")),
        ("Barbados", Place::Code("BB")),
        ("Belarus", Place::Code("BY")),
        ("Belgian Congo", Place::Region(Region::BelgianCongo)),
        ("Belgium", Place::Code("BE")),
        ("Belize", Place::Code("BZ")),
        ("Benelux", Place::Region(Region::Benelux)),
        ("Benin", Place::Code("BJ")),
        ("Bermuda", Place::Code("BM")),
        ("Bhutan", Place::Code("BT")),
        ("Bohemia", Place::Region(Region::Bohemia)),
        ("Bolivia", Place::Code("BO")),
        ("Bosnia & Herzegovina", Place::Code("BA")),
        ("Botswana", Place::Code("BW")),
        ("Brazil", Place::Code("BR")),
        ("British Virgin Islands", Place::Code("VG")),
        ("Brunei", Place::Code("BN")),
        ("Bulgaria", Place::Code("BG")),
        ("Burkina Faso", Place::Code("BF")),
        ("Burma", Place::Code("MM")),
        ("Cambodia", Place::Code("KH")),
        ("Cameroon", Place::Code("CM")),
        ("Canada", Place::Code("CA")),
        ("Cape Verde", Place::Code("CV")),
        ("Cayman Islands", Place::Code("KY")),
        ("Central African Republic", Place::Code("CF")),
        ("Central America", Place::Region(Region::CentralAmerica)),
        ("Chad", Place::Code("TD")),
        ("Chile", Place::Code("CL")),
        ("China", Place::Code("CN")),
        ("Cocos (Keeling) Islands", Place::Code("CC")),
        ("Colombia", Place::Code("CO")),
        ("Comoros", Place::Code("KM")),
        ("Congo, Democratic Republic of the", Place::Code("CD")),
        ("Congo, Republic of the", Place::Code("CG")),
        ("Cook Islands", Place::Code("CK")),
        ("Costa Rica", Place::Code("CR")),
        ("Croatia", Place::Code("HR")),
        ("Cuba", Place::Code("CU")),
        ("Curaçao", Place::Code("CW")),
        ("Cyprus", Place::Code("CY")),
        ("Czech And Slovak Federative Republic", Place::Region(Region::Czechoslovakia)),
        ("Czech Republic", Place::Code("CZ")),
        ("Czech Republic & Slovakia", Place::Region(Region::CzechRepublicAndSlovakia)),
        ("Czechoslovakia", Place::Region(Region::Czechoslovakia)),
        ("Dahomey", Place::Region(Region::Dahomey)),
        ("Denmark", Place::Code("DK")),
        ("Djibouti", Place::Code("DJ")),
        ("Dominica", Place::Code("DM")),
        ("Dominican Republic", Place::Code("DO")),
        ("Dutch East Indies", Place::Region(Region::DutchEastIndies)),
        ("East Timor", Place::Code("TL")),
        ("Ecuador", Place::Code("EC")),
        ("Egypt", Place::Code("EG")),
        ("El Salvador", Place::Code("SV")),
        ("Equatorial Guinea", Place::Code("GQ")),
        ("Eritrea", Place::Code("ER")),
        ("Estonia", Place::Code("EE")),
        ("Ethiopia", Place::Code("ET")),
        ("Europe", Place::Region(Region::Europe)),
        ("Falkland Islands", Place::Code("FK")),
        ("Faroe Islands", Place::Code("FO")),
        ("Fiji", Place::Code("FJ")),
        ("Finland", Place::Code("FI")),
        ("France", Place::Code("FR")),
        ("France & Benelux", Place::Region(Region::FranceAndBenelux)),
        ("French Guiana", Place::Code("GF")),
        ("French Polynesia", Place::Code("PF")),
        ("Gabon", Place::Code("GA")),
        ("Gambia, The", Place::Code("GM")),
        ("Gaza Strip", Place::Code("PS")),
        ("Georgia", Place::Code("GE")),
        ("German Democratic Republic (GDR)", Place::Region(Region::EastGermany)),
        ("Germany", Place::Code("DE")),
        ("Germany & Switzerland", Place::Region(Region::GermanyAndSwitzerland)),
        ("Germany, Austria, & Switzerland", Place::Region(Region::GermanyAustriaAndSwitzerland)),
        ("Ghana", Place::Code("GH")),
        ("Gibraltar", Place::Code("GI")),
        ("Greece", Place::Code("GR")),
        ("Greenland", Place::Code("GL")),
        ("Grenada", Place::Code("GD")),
        ("Guadeloupe", Place::Code("GP")),
        ("Guam", Place::Code("GU")),
        ("Guatemala", Place::Code("GT")),
        ("Guernsey", Place::Code("GG")),
        ("Guinea", Place::Code("GN")),
        ("Guinea-Bissau", Place::Code("GW")),
        ("Gulf Cooperation Council", Place::Region(Region::GulfCooperationCouncil)),
        ("Guyana", Place::Code("GY")),
        ("Haiti", Place::Code("HT")),
        ("Honduras", Place::Code("HN")),
        ("Hong Kong", Place::Code("HK")),
        ("Hong Kong & Thailand", Place::Region(Region::HongKongAndThailand)),
        ("Hungary", Place::Code("HU")),
        ("Iceland", Place::Code("IS")),
        ("India", Place::Code("IN")),
        ("Indochina", Place::Region(Region::Indochina)),
        ("Indonesia", Place::Code("ID")),
        ("Iran", Place::Code("IR")),
        ("Iraq", Place::Code("IQ")),
        ("Ireland", Place::Code("IE")),
        ("Isle Of Man", Place::Code("IM")),
        ("Israel", Place::Code("IL")),
        ("Italian East Africa", Place::Region(Region::ItalianEastAfrica)),
        ("Italy", Place::Code("IT")),
        ("Ivory Coast", Place::Code("CI")),
        ("Jamaica", Place::Code("JM")),
        ("Japan", Place::Code("JP")),
        ("Jersey", Place::Code("JE")),
        ("Jordan", Place::Code("JO")),
        ("Kazakhstan", Place::Code("KZ")),
        ("Kenya", Place::Code("KE")),
        ("Kiribati", Place::Code("KI")),
        ("Korea (pre-1945)", Place::Region(Region::KoreaBefore1945)),
        ("Kosovo", Place::Region(Region::Kosovo)),
        ("Kuwait", Place::Code("KW")),
        ("Kyrgyzstan", Place::Code("KG")),
        ("Laos", Place::Code("LA")),
        ("Latvia", Place::Code("LV")),
        ("Lebanon", Place::Code("LB")),
        ("Lesotho", Place::Code("LS")),
        ("Liberia", Place::Code("LR")),
        ("Libya", Place::Code("LY")),
        ("Liechtenstein", Place::Code("LI")),
        ("Lithuania", Place::Code("LT")),
        ("Luxembourg", Place::Code("LU")),
        ("Macau", Place::Code("MO")),
        ("Macedonia", Place::Code("MK")),
        ("Madagascar", Place::Code("MG")),
        ("Malawi", Place::Code("MW")),
        ("Malaysia", Place::Code("MY")),
        ("Maldives", Place::Code("MV")),
        ("Mali", Place::Code("ML")),
        ("Malta", Place::Code("MT")),
        ("Marshall Islands", Place::Code("MH")),
        ("Martinique", Place::Code("MQ")),
        ("Mauritania", Place::Code("MR")),
        ("Mauritius", Place::Code("MU")),
        ("Mayotte", Place::Code("YT")),
        ("Mexico", Place::Code("MX")),
        ("Middle East", Place::Region(Region::MiddleEast)),
        ("Moldova, Republic of", Place::Code("MD")),
        ("Monaco", Place::Code("MC")),
        ("Mongolia", Place::Code("MN")),
        ("Montenegro", Place::Code("ME")),
        ("Montserrat", Place::Code("MS")),
        ("Morocco", Place::Code("MA")),
        ("Mozambique", Place::Code("MZ")),
        ("Namibia", Place::Code("NA")),
        ("Nauru", Place::Code("NR")),
        ("Nepal", Place::Code("NP")),
        ("Netherlands", Place::Code("NL")),
        ("Netherlands Antilles", Place::Region(Region::NetherlandsAntilles)),
        ("New Caledonia", Place::Code("NC")),
        ("New Zealand", Place::Code("NZ")),
        ("Nicaragua", Place::Code("NI")),
        ("Niger", Place::Code("NE")),
        ("Nigeria", Place::Code("NG")),
        ("Niue", Place::Code("NU")),
        ("Norfolk Island", Place::Code("NF")),
        ("North & South America", Place::Region(Region::NorthAndSouthAmerica)),
        ("North America (inc Mexico)", Place::Region(Region::NorthAmerica)),
        ("North Korea", Place::Code("KP")),
        ("Northern Mariana Islands", Place::Code("MP")),
        ("Norway", Place::Code("NO")),
        ("Oman", Place::Code("OM")),
        ("Ottoman Empire", Place::Region(Region::OttomanEmpire)),
        ("Pakistan", Place::Code("PK")),
        ("Palau", Place::Code("PW")),
        ("Palestine", Place::Code("PS")),
        ("Panama", Place::Code("PA")),
        ("Papua New Guinea", Place::Code("PG")),
        ("Paraguay", Place::Code("PY")),
        ("Peru", Place::Code("PE")),
        ("Philippines", Place::Code("PH")),
        ("Pitcairn Islands", Place::Code("PN")),
        ("Poland", Place::Code("PL")),
        ("Portugal", Place::Code("PT")),
        ("Protectorate of Bohemia and Moravia", Place::Region(Region::ProtectorateOfBohemiaAndMoravia)),
        ("Puerto Rico", Place::Code("PR")),
        ("Qatar", Place::Code("QA")),
        ("Reunion", Place::Code("RE")),
        ("Rhodesia", Place::Region(Region::Rhodesia)),
        ("Romania", Place::Code("RO")),
        ("Russia", Place::Code("RU")),
        ("Russia & CIS", Place::Region(Region::RussiaAndCis)),
        ("Rwanda", Place::Code("RW")),
        ("Saint Kitts and Nevis", Place::Code("KN")),
        ("Saint Lucia", Place::Code("LC")),
        ("Saint Pierre and Miquelon", Place::Code("PM")),
        ("Saint Vincent and the Grenadines", Place::Code("VC")),
        ("Samoa", Place::Code("WS")),
        ("San Marino", Place::Code("SM")),
        ("Sao Tome and Principe", Place::Code("ST")),
        ("Saudi Arabia", Place::Code("SA")),
        ("Scandinavia", Place::Region(Region::Scandinavia)),
        ("Senegal", Place::Code("SN")),
        ("Serbia", Place::Code("RS")),
        ("Serbia and Montenegro", Place::Region(Region::SerbiaAndMontenegro)),
        ("Seychelles", Place::Code("SC")),
        ("Sierra Leone", Place::Code("SL")),
        ("Singapore", Place::Code("SG")),
        ("Singapore & Malaysia", Place::Region(Region::SingaporeAndMalaysia)),
        ("Singapore, Malaysia & Hong Kong", Place::Region(Region::SingaporeMalaysiaAndHongKong)),
        ("Singapore, Malaysia, Hong Kong & Thailand", Place::Region(Region::SingaporeMalaysiaHongKongAndThailand)),
        ("Sint Maarten", Place::Code("SX")),
        ("Slovakia", Place::Code("SK")),
        ("Slovenia", Place::Code("SI")),
        ("Solomon Islands", Place::Code("SB")),
        ("Somalia", Place::Code("SO")),
        ("South Africa", Place::Code("ZA")),
        ("South America", Place::Region(Region::SouthAmerica)),
        ("South East Asia", Place::Region(Region::SouthEastAsia)),
        ("South Korea", Place::Code("KR")),
        ("South Pacific", Place::Region(Region::SouthPacific)),
        ("South Vietnam", Place::Region(Region::SouthVietnam)),
        ("South West Africa", Place::Region(Region::SouthWestAfrica)),
        ("Southern Rhodesia", Place::Region(Region::SouthernRhodesia)),
        ("Southern Sudan", Place::Code("SS")),
        ("Spain", Place::Code("ES")),
        ("Sri Lanka", Place::Code("LK")),
        ("Sudan", Place::Code("SD")),
        ("Suriname", Place::Code("SR")),
        ("Swaziland", Place::Code("SZ")),
        ("Sweden", Place::Code("SE")),
        ("Switzerland", Place::Code("CH")),
        ("Syria", Place::Code("SY")),
        ("Taiwan", Place::Code("TW")),
        ("Tajikistan", Place::Code("TJ")),
        ("Tanzania", Place::Code("TZ")),
        ("Thailand", Place::Code("TH")),
        ("Togo", Place::Code("TG")),
        ("Tonga", Place::Code("TO")),
        ("Trinidad & Tobago", Place::Code("TT")),
        ("Tunisia", Place::Code("TN")),
        ("Turkey", Place::Code("TR")),
        ("Turkmenistan", Place::Code("TM")),
        ("Turks and Caicos Islands", Place::Code("TC")),
        ("Uganda", Place::Code("UG")),
        ("UK", Place::Code("GB")),
        ("UK & Europe", Place::Region(Region::UkAndEurope)),
        ("UK & France", Place::Region(Region::UkAndFrance)),
        ("UK & Germany", Place::Region(Region::UkAndGermany)),
        ("UK & Ireland", Place::Region(Region::UkAndIreland)),
        ("UK & US", Place::Region(Region::UkAndUs)),
        ("UK, Europe & Israel", Place::Region(Region::UkEuropeAndIsrael)),
        ("UK, Europe & Japan", Place::Region(Region::UkEuropeAndJapan)),
        ("UK, Europe & US", Place::Region(Region::UkEuropeAndUs)),
        ("Ukraine", Place::Code("UA")),
        ("United Arab Emirates", Place::Code("AE")),
        ("Upper Volta", Place::Region(Region::UpperVolta)),
        ("Uruguay", Place::Code("UY")),
        ("US", Place::Code("US")),
        ("USA & Canada", Place::Region(Region::UsaAndCanada)),
        ("USA & Europe", Place::Region(Region::UsaAndEurope)),
        ("USA, Canada & Europe", Place::Region(Region::UsaCanadaAndEurope)),
        ("USA, Canada & UK", Place::Region(Region::UsaCanadaAndUk)),
        ("USSR", Place::Region(Region::SovietUnion)),
        ("Uzbekistan", Place::Code("UZ")),
        ("Vanuatu", Place::Code("VU")),
        ("Vatican City", Place::Code("VA")),
        ("Venezuela", Place::Code("VE")),
        ("Vietnam", Place::Code("VN")),
        ("Virgin Islands", Place::Code("VI")),
        ("Wallis and Futuna", Place::Code("WF")),
        ("West Bank", Place::Code("PS")),
        ("Worldwide", Place::Region(Region::Worldwide)),
        ("Yemen", Place::Code("YE")),
        ("Yugoslavia", Place::Region(Region::Yugoslavia)),
        ("Zaire", Place::Region(Region::Zaire)),
        ("Zambia", Place::Code("ZM")),
        ("Zanzibar", Place::Region(Region::Zanzibar)),
        ("Zimbabwe", Place::Code("ZW")),
    ];
}

#[cfg(test)]
mod tests {
    use super::super::recorded;
    use super::*;

    #[test]
    fn the_discogs_table_is_discogss_own_list() {
        assert_eq!(
            DISCOGS_COUNTRIES
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
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
        let page = include_str!(
            "../../test-fixtures/pressing-vocabulary/musicbrainz-release-countries.txt"
        );
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
            MUSICBRAINZ_REGION_CODES
                .iter()
                .map(|(code, _)| *code)
                .collect::<Vec<_>>()
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

    #[test]
    fn both_catalogs_name_the_same_areas_alike() {
        let japan = ReleaseArea::Country(Country::from_code("JP").unwrap());
        assert_eq!(ReleaseArea::musicbrainz("JP"), Some(japan));
        assert_eq!(ReleaseArea::discogs("Japan"), Some(japan));
        assert_eq!(
            ReleaseArea::discogs("UK"),
            Some(ReleaseArea::Country(Country::from_code("GB").unwrap()))
        );
        assert_eq!(
            ReleaseArea::musicbrainz("XE"),
            ReleaseArea::discogs("Europe")
        );
        assert_eq!(ReleaseArea::musicbrainz("SU"), ReleaseArea::discogs("USSR"));
        assert_eq!(
            ReleaseArea::discogs("uk & europe"),
            Some(ReleaseArea::Region(Region::UkAndEurope))
        );
        assert_eq!(ReleaseArea::discogs("Atlantis"), None);
        assert_eq!(ReleaseArea::musicbrainz("ZZ"), None);
    }
}
