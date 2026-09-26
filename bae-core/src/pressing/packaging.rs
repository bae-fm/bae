//! What a release is sold in.
//!
//! MusicBrainz states a release's outermost packaging from a closed list; the
//! list is reproduced here name for name. Discogs has no packaging field: its
//! one word for packaging is the "Box Set" format, which says the media are
//! enclosed in a box, and that is read as [`Packaging::Box`].

super::vocabulary! {
    /// The outermost packaging of a release, leaving out shipping material.
    pub enum Packaging {
        JewelCase => "jewel_case",
        SlimJewelCase => "slim_jewel_case",
        Digipak => "digipak",
        CardboardSleeve => "cardboard_sleeve",
        /// A packaging MusicBrainz's list has no name for.
        Other => "other",
        KeepCase => "keep_case",
        /// Sold with no packaging at all, as a download is.
        Unpackaged => "unpackaged",
        CassetteCase => "cassette_case",
        Book => "book",
        Fatbox => "fatbox",
        SnapCase => "snap_case",
        GatefoldCover => "gatefold_cover",
        DiscboxSlider => "discbox_slider",
        SuperJewelBox => "super_jewel_box",
        Digibook => "digibook",
        PlasticSleeve => "plastic_sleeve",
        Box => "box",
        Slidepack => "slidepack",
        SnapPack => "snap_pack",
        MetalTin => "metal_tin",
        Longbox => "longbox",
        ClamshellCase => "clamshell_case",
        Digifile => "digifile",
        Slipcase => "slipcase",
    }
}

desktop_only! {
    impl Packaging {
        /// The packaging a MusicBrainz release's `packaging` names. `None` for a
        /// name outside the list, which the caller logs.
        pub(crate) fn musicbrainz(name: &str) -> Option<Self> {
            MUSICBRAINZ_RELEASE_PACKAGING
                .iter()
                .find(|(stated, _)| super::same_name(stated, name))
                .map(|(_, packaging)| *packaging)
        }
    }

    /// Every `release_packaging` row of MusicBrainz, in id order. Source: the
    /// server's attribute translations (`po/attributes.pot`, generated from the
    /// `release_packaging` table), captured 2026-09-25. The web service answers
    /// with these names.
    const MUSICBRAINZ_RELEASE_PACKAGING: &[(&str, Packaging)] = &[
        ("Jewel Case", Packaging::JewelCase),
        ("Slim Jewel Case", Packaging::SlimJewelCase),
        ("Digipak", Packaging::Digipak),
        ("Cardboard/Paper Sleeve", Packaging::CardboardSleeve),
        ("Other", Packaging::Other),
        ("Keep Case", Packaging::KeepCase),
        ("None", Packaging::Unpackaged),
        ("Cassette Case", Packaging::CassetteCase),
        ("Book", Packaging::Book),
        ("Fatbox", Packaging::Fatbox),
        ("Snap Case", Packaging::SnapCase),
        ("Gatefold Cover", Packaging::GatefoldCover),
        ("Discbox Slider", Packaging::DiscboxSlider),
        ("Super Jewel Box", Packaging::SuperJewelBox),
        ("Digibook", Packaging::Digibook),
        ("Plastic Sleeve", Packaging::PlasticSleeve),
        ("Box", Packaging::Box),
        ("Slidepack", Packaging::Slidepack),
        ("SnapPack", Packaging::SnapPack),
        ("Metal Tin", Packaging::MetalTin),
        ("Longbox", Packaging::Longbox),
        ("Clamshell Case", Packaging::ClamshellCase),
        ("Digifile", Packaging::Digifile),
        ("Slipcase", Packaging::Slipcase),
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_musicbrainzs_own_list() {
        assert_eq!(
            MUSICBRAINZ_RELEASE_PACKAGING
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            super::super::recorded(include_str!(
                "../../test-fixtures/pressing-vocabulary/musicbrainz-release-packaging.txt"
            ))
        );
        let named: Vec<_> = MUSICBRAINZ_RELEASE_PACKAGING
            .iter()
            .map(|(_, packaging)| *packaging)
            .collect();
        assert_eq!(named, Packaging::ALL, "one packaging per MusicBrainz name");
    }

    #[test]
    fn none_is_a_packaging_rather_than_an_unstated_one() {
        assert_eq!(Packaging::musicbrainz("None"), Some(Packaging::Unpackaged));
        assert_eq!(
            Packaging::musicbrainz("cardboard/paper sleeve"),
            Some(Packaging::CardboardSleeve)
        );
        assert_eq!(Packaging::musicbrainz("Digisleeve"), None);
    }
}
