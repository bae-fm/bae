//! What Discogs says about a pressing that neither catalog has a field for.
//!
//! A Discogs format entry carries descriptions from a closed list: a size, a
//! speed, a file type, "Reissue", "Limited Edition". Some of them are facts
//! bae has a field for — "Promo" is a status, "DualDisc" under a "Hybrid"
//! format names the carrier — and some name what kind of release it is
//! ("Album", "Single") rather than anything about the pressing, which bae
//! reads from the release group instead and drops here. The rest are
//! [`DiscogsDetail`]s: the list's own words, kept as the source's detail and
//! shown the same way wherever they appear.
//!
//! The list is Discogs's, description for description, and the test that
//! compares the table with the captured page keeps them together.

/// How a detail is shown: in the reader's language, or as Discogs prints it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wording {
    /// A word a person reads — "Reissue", "Limited Edition" — which the
    /// surfaces translate.
    Word,
    /// A term that reads the same in every language: a size, a speed, a
    /// file type, a standard's name.
    Term,
}

/// Shorthand for the detail list: each variant's stored key, the term
/// Discogs prints, and how the term is shown.
macro_rules! discogs_details {
    ($($variant:ident => ($key:literal, $term:literal, $wording:ident)),+ $(,)?) => {
        super::vocabulary! {
            /// One of Discogs's format descriptions that says something about
            /// the pressing no field of bae's holds.
            pub enum DiscogsDetail {
                $($variant => $key),+
            }
        }

        impl DiscogsDetail {
            /// The description as Discogs prints it.
            pub fn term(self) -> &'static str {
                match self {
                    $(Self::$variant => $term),+
                }
            }

            /// Whether the term is translated or shown as printed.
            pub fn wording(self) -> Wording {
                match self {
                    $(Self::$variant => Wording::$wording),+
                }
            }
        }
    };
}

discogs_details! {
        Lp => ("lp", "LP", Term),
        Size16In => ("size16_in", "16\"", Term),
        Size12In => ("size12_in", "12\"", Term),
        Size14In => ("size14_in", "14\"", Term),
        Size11In => ("size11_in", "11\"", Term),
        Size10In => ("size10_in", "10\"", Term),
        Size9In => ("size9_in", "9\"", Term),
        Size8In => ("size8_in", "8\"", Term),
        Size7In => ("size7_in", "7\"", Term),
        Size6HalfIn => ("size6_half_in", "6½\"", Term),
        Size6In => ("size6_in", "6\"", Term),
        Size5HalfIn => ("size5_half_in", "5½\"", Term),
        Size5In => ("size5_in", "5\"", Term),
        Size4In => ("size4_in", "4\"", Term),
        Size3HalfIn => ("size3_half_in", "3½\"", Term),
        Size3In => ("size3_in", "3\"", Term),
        Size2In => ("size2_in", "2\"", Term),
        Size1In => ("size1_in", "1\"", Term),
        Rpm8Third => ("rpm8_third", "8 ⅓ RPM", Term),
        Rpm16TwoThirds => ("rpm16_two_thirds", "16 ⅔ RPM", Term),
        Rpm33Third => ("rpm33_third", "33 ⅓ RPM", Term),
        Rpm45 => ("rpm45", "45 RPM", Term),
        Rpm78 => ("rpm78", "78 RPM", Term),
        Rpm120 => ("rpm120", "120 RPM", Term),
        Size21Cm => ("size21_cm", "21cm", Term),
        Size25Cm => ("size25_cm", "25cm", Term),
        Size27Cm => ("size27_cm", "27cm", Term),
        Size29Cm => ("size29_cm", "29cm", Term),
        Size35Cm => ("size35_cm", "35cm", Term),
        Size40Cm => ("size40_cm", "40cm", Term),
        Size50Cm => ("size50_cm", "50cm", Term),
        Rpm80 => ("rpm80", "80 RPM", Term),
        Rpm90 => ("rpm90", "90 RPM", Term),
        IpsFifteenSixteenths => ("ips_fifteen_sixteenths", "15/16 ips", Term),
        Ips1SevenEighths => ("ips1_seven_eighths", "1 ⅞ ips", Term),
        Ips15 => ("ips15", "15 ips", Term),
        Ips3ThreeQuarters => ("ips3_three_quarters", "3 ¾ ips", Term),
        Ips30 => ("ips30", "30 ips", Term),
        Ips7Half => ("ips7_half", "7 ½ ips", Term),
        HalfInchTape => ("half_inch_tape", "½\"", Term),
        QuarterInchTape => ("quarter_inch_tape", "¼\"", Term),
        EighthInchTape => ("eighth_inch_tape", "⅛\"", Term),
        T2TrackMono => ("t2_track_mono", "2-Track Mono", Term),
        T2TrackStereo => ("t2_track_stereo", "2-Track Stereo", Term),
        T4TrackMono => ("t4_track_mono", "4-Track Mono", Term),
        T4TrackStereo => ("t4_track_stereo", "4-Track Stereo", Term),
        NabReel10Point5In => ("nab_reel10_point5_in", "10.5\" NAB Reel", Term),
        CineReel3In => ("cine_reel3_in", "3\" Cine Reel", Term),
        CineReel5In => ("cine_reel5_in", "5\" Cine Reel", Term),
        CineReel6In => ("cine_reel6_in", "6\" Cine Reel", Term),
        CineReel7In => ("cine_reel7_in", "7\" Cine Reel", Term),
        Minute2 => ("minute2", "2 Minute", Term),
        Minute3 => ("minute3", "3 Minute", Term),
        Minute4 => ("minute4", "4 Minute", Term),
        Concert => ("concert", "Concert", Term),
        Salon => ("salon", "Salon", Term),
        Mini => ("mini", "Mini", Term),
        BusinessCard => ("business_card", "Business Card", Word),
        Shape => ("shape", "Shape", Word),
        Minimax => ("minimax", "Minimax", Term),
        CdRom => ("cd_rom", "CD-ROM", Term),
        CdI => ("cd_i", "CDi", Term),
        CdPlusG => ("cd_plus_g", "CD+G", Term),
        Hdcd => ("hdcd", "HDCD", Term),
        Vcd => ("vcd", "VCD", Term),
        Avcd => ("avcd", "AVCD", Term),
        Svcd => ("svcd", "SVCD", Term),
        Xrcd => ("xrcd", "XRCD", Term),
        Uhd4k => ("uhd4k", "4K", Term),
        Uhd8k => ("uhd8k", "8K", Term),
        BluRayAudio => ("blu_ray_audio", "Blu-ray Audio", Term),
        Multichannel => ("multichannel", "Multichannel", Word),
        DvdAudio => ("dvd_audio", "DVD-Audio", Term),
        DvdData => ("dvd_data", "DVD-Data", Term),
        DvdVideo => ("dvd_video", "DVD-Video", Term),
        Hybrid => ("hybrid", "Hybrid", Word),
        Aac => ("aac", "AAC", Term),
        Aifc => ("aifc", "AIFC", Term),
        Aiff => ("aiff", "AIFF", Term),
        Alac => ("alac", "ALAC", Term),
        Amr => ("amr", "AMR", Term),
        Ape => ("ape", "APE", Term),
        Avi => ("avi", "AVI", Term),
        Dff => ("dff", "DFF", Term),
        DiscImage => ("disc_image", "Disc Image", Word),
        Dsf => ("dsf", "DSF", Term),
        Flac => ("flac", "FLAC", Term),
        Flv => ("flv", "FLV", Term),
        Mov => ("mov", "MOV", Term),
        Mp1 => ("mp1", "MP1", Term),
        Mp2 => ("mp2", "MP2", Term),
        Mp3 => ("mp3", "MP3", Term),
        MpegVideo => ("mpeg_video", "MPEG Video", Term),
        Mpeg4Video => ("mpeg4_video", "MPEG-4 Video", Term),
        OggVorbis => ("ogg_vorbis", "ogg-vorbis", Term),
        Opus => ("opus", "Opus", Term),
        Ra => ("ra", "RA", Term),
        Rm => ("rm", "RM", Term),
        Shn => ("shn", "SHN", Term),
        Spx => ("spx", "SPX", Term),
        Swf => ("swf", "SWF", Term),
        Tta => ("tta", "TTA", Term),
        Wav => ("wav", "WAV", Term),
        WavPack => ("wav_pack", "WavPack", Term),
        Wma => ("wma", "WMA", Term),
        Wmv => ("wmv", "WMV", Term),
        Mp3Surround => ("mp3_surround", "MP3 Surround", Term),
        Floppy3Point5In => ("floppy3_point5_in", "3.5\"", Term),
        Floppy5Point25In => ("floppy5_point25_in", "5.25\"", Term),
        DoubleSided => ("double_sided", "Double Sided", Word),
        SingleSided => ("single_sided", "Single Sided", Word),
        Advance => ("advance", "Advance", Word),
        RecordStoreDay => ("record_store_day", "Record Store Day", Term),
        Stereo => ("stereo", "Stereo", Word),
        Mono => ("mono", "Mono", Word),
        Quadraphonic => ("quadraphonic", "Quadraphonic", Word),
        Ambisonic => ("ambisonic", "Ambisonic", Word),
        Bioplastic => ("bioplastic", "Bioplastic", Word),
        CardBacked => ("card_backed", "Card Backed", Word),
        ClubEdition => ("club_edition", "Club Edition", Word),
        CopyProtected => ("copy_protected", "Copy Protected", Word),
        DeluxeEdition => ("deluxe_edition", "Deluxe Edition", Word),
        Enhanced => ("enhanced", "Enhanced", Word),
        Etched => ("etched", "Etched", Word),
        Jukebox => ("jukebox", "Jukebox", Word),
        LimitedEdition => ("limited_edition", "Limited Edition", Word),
        Mispress => ("mispress", "Mispress", Word),
        Misprint => ("misprint", "Misprint", Word),
        Numbered => ("numbered", "Numbered", Word),
        PartiallyMixed => ("partially_mixed", "Partially Mixed", Word),
        PartiallyUnofficial => ("partially_unofficial", "Partially Unofficial", Word),
        PictureDisc => ("picture_disc", "Picture Disc", Word),
        Reissue => ("reissue", "Reissue", Word),
        Remastered => ("remastered", "Remastered", Word),
        Repress => ("repress", "Repress", Word),
        SpecialCut => ("special_cut", "Special Cut", Word),
        SpecialEdition => ("special_edition", "Special Edition", Word),
        Styrene => ("styrene", "Styrene", Word),
        TestPressing => ("test_pressing", "Test Pressing", Word),
        TourRecording => ("tour_recording", "Tour Recording", Word),
        Transcription => ("transcription", "Transcription", Word),
        WhiteLabel => ("white_label", "White Label", Word),
        Film16mm => ("film16mm", "16mm", Term),
        Film35mm => ("film35mm", "35mm", Term),
        Ntsc => ("ntsc", "NTSC", Term),
        Pal => ("pal", "PAL", Term),
        Secam => ("secam", "SECAM", Term),
}

desktop_only! {
    use super::{Medium, ReleaseStatus};

    /// What one Discogs format description says.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Role {
        /// The kind of release — an album, a single, a compilation — which
        /// says nothing about the pressing.
        ReleaseType,
        /// The release's status.
        Status(ReleaseStatus),
        /// The carrier of a "Hybrid" format entry, which names no carrier of
        /// its own. Under any other format it restates what the format name
        /// already says.
        HybridCarrier(Medium),
        Detail(DiscogsDetail),
    }

    /// What a Discogs format description says. `None` for a description
    /// outside the list, which the caller logs: the list is what needs it.
    pub(crate) fn role(description: &str) -> Option<Role> {
        DISCOGS_FORMAT_DESCRIPTIONS
            .iter()
            .find(|(stated, _)| super::same_name(stated, description))
            .map(|(_, role)| *role)
    }

    /// The descriptions of Discogs, from the "The Description Field" table of
    /// <https://www.discogs.com/help/formatslist> (read through
    /// <https://web.archive.org/web/20260103075003/https://www.discogs.com/help/formatslist>,
    /// captured 2026-01-03), in page order with the repeats dropped — the page
    /// lists a description again under each format it applies to.
    const DISCOGS_FORMAT_DESCRIPTIONS: &[(&str, Role)] = &[
        ("LP", Role::Detail(DiscogsDetail::Lp)),
        ("16\"", Role::Detail(DiscogsDetail::Size16In)),
        ("12\"", Role::Detail(DiscogsDetail::Size12In)),
        ("14\"", Role::Detail(DiscogsDetail::Size14In)),
        ("11\"", Role::Detail(DiscogsDetail::Size11In)),
        ("10\"", Role::Detail(DiscogsDetail::Size10In)),
        ("9\"", Role::Detail(DiscogsDetail::Size9In)),
        ("8\"", Role::Detail(DiscogsDetail::Size8In)),
        ("7\"", Role::Detail(DiscogsDetail::Size7In)),
        ("6½\"", Role::Detail(DiscogsDetail::Size6HalfIn)),
        ("6\"", Role::Detail(DiscogsDetail::Size6In)),
        ("5½\"", Role::Detail(DiscogsDetail::Size5HalfIn)),
        ("5\"", Role::Detail(DiscogsDetail::Size5In)),
        ("4\"", Role::Detail(DiscogsDetail::Size4In)),
        ("3½\"", Role::Detail(DiscogsDetail::Size3HalfIn)),
        ("3\"", Role::Detail(DiscogsDetail::Size3In)),
        ("2\"", Role::Detail(DiscogsDetail::Size2In)),
        ("1\"", Role::Detail(DiscogsDetail::Size1In)),
        ("8 ⅓ RPM", Role::Detail(DiscogsDetail::Rpm8Third)),
        ("16 ⅔ RPM", Role::Detail(DiscogsDetail::Rpm16TwoThirds)),
        ("33 ⅓ RPM", Role::Detail(DiscogsDetail::Rpm33Third)),
        ("45 RPM", Role::Detail(DiscogsDetail::Rpm45)),
        ("78 RPM", Role::Detail(DiscogsDetail::Rpm78)),
        ("120 RPM", Role::Detail(DiscogsDetail::Rpm120)),
        ("21cm", Role::Detail(DiscogsDetail::Size21Cm)),
        ("25cm", Role::Detail(DiscogsDetail::Size25Cm)),
        ("27cm", Role::Detail(DiscogsDetail::Size27Cm)),
        ("29cm", Role::Detail(DiscogsDetail::Size29Cm)),
        ("35cm", Role::Detail(DiscogsDetail::Size35Cm)),
        ("40cm", Role::Detail(DiscogsDetail::Size40Cm)),
        ("50cm", Role::Detail(DiscogsDetail::Size50Cm)),
        ("80 RPM", Role::Detail(DiscogsDetail::Rpm80)),
        ("90 RPM", Role::Detail(DiscogsDetail::Rpm90)),
        ("15/16 ips", Role::Detail(DiscogsDetail::IpsFifteenSixteenths)),
        ("1 ⅞ ips", Role::Detail(DiscogsDetail::Ips1SevenEighths)),
        ("15 ips", Role::Detail(DiscogsDetail::Ips15)),
        ("3 ¾ ips", Role::Detail(DiscogsDetail::Ips3ThreeQuarters)),
        ("30 ips", Role::Detail(DiscogsDetail::Ips30)),
        ("7 ½ ips", Role::Detail(DiscogsDetail::Ips7Half)),
        ("½\"", Role::Detail(DiscogsDetail::HalfInchTape)),
        ("¼\"", Role::Detail(DiscogsDetail::QuarterInchTape)),
        ("⅛\"", Role::Detail(DiscogsDetail::EighthInchTape)),
        ("2-Track Mono", Role::Detail(DiscogsDetail::T2TrackMono)),
        ("2-Track Stereo", Role::Detail(DiscogsDetail::T2TrackStereo)),
        ("4-Track Mono", Role::Detail(DiscogsDetail::T4TrackMono)),
        ("4-Track Stereo", Role::Detail(DiscogsDetail::T4TrackStereo)),
        ("10.5\" NAB Reel", Role::Detail(DiscogsDetail::NabReel10Point5In)),
        ("3\" Cine Reel", Role::Detail(DiscogsDetail::CineReel3In)),
        ("5\" Cine Reel", Role::Detail(DiscogsDetail::CineReel5In)),
        ("6\" Cine Reel", Role::Detail(DiscogsDetail::CineReel6In)),
        ("7\" Cine Reel", Role::Detail(DiscogsDetail::CineReel7In)),
        ("2 Minute", Role::Detail(DiscogsDetail::Minute2)),
        ("3 Minute", Role::Detail(DiscogsDetail::Minute3)),
        ("4 Minute", Role::Detail(DiscogsDetail::Minute4)),
        ("Concert", Role::Detail(DiscogsDetail::Concert)),
        ("Salon", Role::Detail(DiscogsDetail::Salon)),
        ("Mini", Role::Detail(DiscogsDetail::Mini)),
        ("Business Card", Role::Detail(DiscogsDetail::BusinessCard)),
        ("Shape", Role::Detail(DiscogsDetail::Shape)),
        ("Minimax", Role::Detail(DiscogsDetail::Minimax)),
        ("CD-ROM", Role::Detail(DiscogsDetail::CdRom)),
        ("CDi", Role::Detail(DiscogsDetail::CdI)),
        ("CD+G", Role::Detail(DiscogsDetail::CdPlusG)),
        ("HDCD", Role::Detail(DiscogsDetail::Hdcd)),
        ("VCD", Role::Detail(DiscogsDetail::Vcd)),
        ("AVCD", Role::Detail(DiscogsDetail::Avcd)),
        ("SVCD", Role::Detail(DiscogsDetail::Svcd)),
        ("XRCD", Role::Detail(DiscogsDetail::Xrcd)),
        ("4K", Role::Detail(DiscogsDetail::Uhd4k)),
        ("8K", Role::Detail(DiscogsDetail::Uhd8k)),
        ("Blu-ray Audio", Role::Detail(DiscogsDetail::BluRayAudio)),
        ("Multichannel", Role::Detail(DiscogsDetail::Multichannel)),
        ("DVD-Audio", Role::Detail(DiscogsDetail::DvdAudio)),
        ("DVD-Data", Role::Detail(DiscogsDetail::DvdData)),
        ("DVD-Video", Role::Detail(DiscogsDetail::DvdVideo)),
        ("Hybrid", Role::Detail(DiscogsDetail::Hybrid)),
        ("AAC", Role::Detail(DiscogsDetail::Aac)),
        ("AIFC", Role::Detail(DiscogsDetail::Aifc)),
        ("AIFF", Role::Detail(DiscogsDetail::Aiff)),
        ("ALAC", Role::Detail(DiscogsDetail::Alac)),
        ("AMR", Role::Detail(DiscogsDetail::Amr)),
        ("APE", Role::Detail(DiscogsDetail::Ape)),
        ("AVI", Role::Detail(DiscogsDetail::Avi)),
        ("DFF", Role::Detail(DiscogsDetail::Dff)),
        ("Disc Image", Role::Detail(DiscogsDetail::DiscImage)),
        ("DSF", Role::Detail(DiscogsDetail::Dsf)),
        ("FLAC", Role::Detail(DiscogsDetail::Flac)),
        ("FLV", Role::Detail(DiscogsDetail::Flv)),
        ("MOV", Role::Detail(DiscogsDetail::Mov)),
        ("MP1", Role::Detail(DiscogsDetail::Mp1)),
        ("MP2", Role::Detail(DiscogsDetail::Mp2)),
        ("MP3", Role::Detail(DiscogsDetail::Mp3)),
        ("MPEG Video", Role::Detail(DiscogsDetail::MpegVideo)),
        ("MPEG-4 Video", Role::Detail(DiscogsDetail::Mpeg4Video)),
        ("ogg-vorbis", Role::Detail(DiscogsDetail::OggVorbis)),
        ("Opus", Role::Detail(DiscogsDetail::Opus)),
        ("RA", Role::Detail(DiscogsDetail::Ra)),
        ("RM", Role::Detail(DiscogsDetail::Rm)),
        ("SHN", Role::Detail(DiscogsDetail::Shn)),
        ("SPX", Role::Detail(DiscogsDetail::Spx)),
        ("SWF", Role::Detail(DiscogsDetail::Swf)),
        ("TTA", Role::Detail(DiscogsDetail::Tta)),
        ("WAV", Role::Detail(DiscogsDetail::Wav)),
        ("WavPack", Role::Detail(DiscogsDetail::WavPack)),
        ("WMA", Role::Detail(DiscogsDetail::Wma)),
        ("WMV", Role::Detail(DiscogsDetail::Wmv)),
        ("MP3 Surround", Role::Detail(DiscogsDetail::Mp3Surround)),
        ("3.5\"", Role::Detail(DiscogsDetail::Floppy3Point5In)),
        ("5.25\"", Role::Detail(DiscogsDetail::Floppy5Point25In)),
        ("CD-Record", Role::HybridCarrier(Medium::CdRecord)),
        ("DualDisc", Role::HybridCarrier(Medium::DualDisc)),
        ("DVDplus", Role::HybridCarrier(Medium::DvdPlus)),
        ("VinylDisc", Role::HybridCarrier(Medium::VinylDisc)),
        ("Double Sided", Role::Detail(DiscogsDetail::DoubleSided)),
        ("Single Sided", Role::Detail(DiscogsDetail::SingleSided)),
        ("Advance", Role::Detail(DiscogsDetail::Advance)),
        ("Album", Role::ReleaseType),
        ("Mini-Album", Role::ReleaseType),
        ("EP", Role::ReleaseType),
        ("Maxi-Single", Role::ReleaseType),
        ("Record Store Day", Role::Detail(DiscogsDetail::RecordStoreDay)),
        ("Single", Role::ReleaseType),
        ("Compilation", Role::ReleaseType),
        ("Stereo", Role::Detail(DiscogsDetail::Stereo)),
        ("Mono", Role::Detail(DiscogsDetail::Mono)),
        ("Quadraphonic", Role::Detail(DiscogsDetail::Quadraphonic)),
        ("Ambisonic", Role::Detail(DiscogsDetail::Ambisonic)),
        ("Bioplastic", Role::Detail(DiscogsDetail::Bioplastic)),
        ("Card Backed", Role::Detail(DiscogsDetail::CardBacked)),
        ("Club Edition", Role::Detail(DiscogsDetail::ClubEdition)),
        ("Copy Protected", Role::Detail(DiscogsDetail::CopyProtected)),
        ("Deluxe Edition", Role::Detail(DiscogsDetail::DeluxeEdition)),
        ("Enhanced", Role::Detail(DiscogsDetail::Enhanced)),
        ("Etched", Role::Detail(DiscogsDetail::Etched)),
        ("Jukebox", Role::Detail(DiscogsDetail::Jukebox)),
        ("Limited Edition", Role::Detail(DiscogsDetail::LimitedEdition)),
        ("Mispress", Role::Detail(DiscogsDetail::Mispress)),
        ("Misprint", Role::Detail(DiscogsDetail::Misprint)),
        ("Mixed", Role::ReleaseType),
        ("Mixtape", Role::ReleaseType),
        ("Numbered", Role::Detail(DiscogsDetail::Numbered)),
        ("Partially Mixed", Role::Detail(DiscogsDetail::PartiallyMixed)),
        ("Partially Unofficial", Role::Detail(DiscogsDetail::PartiallyUnofficial)),
        ("Picture Disc", Role::Detail(DiscogsDetail::PictureDisc)),
        ("Promo", Role::Status(ReleaseStatus::Promotion)),
        ("Reissue", Role::Detail(DiscogsDetail::Reissue)),
        ("Remastered", Role::Detail(DiscogsDetail::Remastered)),
        ("Repress", Role::Detail(DiscogsDetail::Repress)),
        ("Sampler", Role::ReleaseType),
        ("Special Cut", Role::Detail(DiscogsDetail::SpecialCut)),
        ("Special Edition", Role::Detail(DiscogsDetail::SpecialEdition)),
        ("Styrene", Role::Detail(DiscogsDetail::Styrene)),
        ("Test Pressing", Role::Detail(DiscogsDetail::TestPressing)),
        ("Tour Recording", Role::Detail(DiscogsDetail::TourRecording)),
        ("Transcription", Role::Detail(DiscogsDetail::Transcription)),
        ("Unofficial Release", Role::Status(ReleaseStatus::Bootleg)),
        ("White Label", Role::Detail(DiscogsDetail::WhiteLabel)),
        ("16mm", Role::Detail(DiscogsDetail::Film16mm)),
        ("35mm", Role::Detail(DiscogsDetail::Film35mm)),
        ("NTSC", Role::Detail(DiscogsDetail::Ntsc)),
        ("PAL", Role::Detail(DiscogsDetail::Pal)),
        ("SECAM", Role::Detail(DiscogsDetail::Secam)),
    ];
}

#[cfg(test)]
mod tests {
    use super::super::recorded;
    use super::*;

    #[test]
    fn the_table_is_discogss_own_list() {
        assert_eq!(
            DISCOGS_FORMAT_DESCRIPTIONS
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            recorded(include_str!(
                "../../test-fixtures/pressing-vocabulary/discogs-format-descriptions.txt"
            ))
        );
    }

    /// Each detail is one description of the list, printed as Discogs prints
    /// it, and every description of the list that is a detail is one.
    #[test]
    fn every_detail_is_one_description() {
        let details: Vec<DiscogsDetail> = DISCOGS_FORMAT_DESCRIPTIONS
            .iter()
            .filter_map(|(term, role)| match role {
                Role::Detail(detail) => {
                    assert_eq!(detail.term(), *term);
                    Some(*detail)
                }
                _ => None,
            })
            .collect();
        assert_eq!(details, DiscogsDetail::ALL);
    }

    #[test]
    fn a_description_is_read_whole_and_in_any_case() {
        assert_eq!(role("reissue"), Some(Role::Detail(DiscogsDetail::Reissue)));
        assert_eq!(role("Promo"), Some(Role::Status(ReleaseStatus::Promotion)));
        assert_eq!(role("Album"), Some(Role::ReleaseType));
        assert_eq!(
            role("DualDisc"),
            Some(Role::HybridCarrier(Medium::DualDisc))
        );
        assert_eq!(role("180 Gram"), None);
    }
}
