//! The carriers the two catalogs name, in their own words.
//!
//! MusicBrainz and Discogs each state a release's media from a closed list:
//! MusicBrainz names one format per medium, Discogs names one format per
//! entry of the release's `formats`, with a quantity and its descriptions.
//! Both format lists are reproduced below, name for name, and each name is
//! given the carrier family a person would call a different object — a CD-R
//! and an SHM-CD are both a CD, a File is a download, and a cassette is
//! neither. The Discogs descriptions are read by
//! [`crate::pressing::discogs_detail`].
//!
//! The lists are the vocabulary: a name outside them is not a medium this
//! module knows, and the test that compares each table against its captured
//! record is what keeps the code and the record from drifting apart.

use super::PhysicalMedium;

super::vocabulary! {
    /// A carrier family: what the physical (or downloaded) object a pressing is
    /// made of is, as far as two records of it can be told apart. One variant
    /// per family a person would call a different object; the sizes, speeds,
    /// pressing plants and layer counts the catalogs also name are the same
    /// object.
    pub enum Medium {
        Cd => "cd",
        Sacd => "sacd",
        Dvd => "dvd",
        HdDvd => "hd_dvd",
        BluRay => "blu_ray",
        Vinyl => "vinyl",
        Shellac => "shellac",
        Acetate => "acetate",
        Cassette => "cassette",
        /// Digital Compact Cassette.
        Dcc => "dcc",
        /// Digital Audio Tape.
        Dat => "dat",
        /// A tape cartridge: the 4- and 8-track families and the players that
        /// took their own cartridge.
        Cartridge => "cartridge",
        ReelToReel => "reel_to_reel",
        /// A download or a card that carries one, which is a copy rather than an
        /// object.
        Digital => "digital",
        MiniDisc => "mini_disc",
        LaserDisc => "laser_disc",
        /// A CD carrying an analogue video track.
        Cdv => "cdv",
        /// Video CD and its super form.
        VideoCd => "video_cd",
        /// A video cassette, whichever of the consumer and broadcast families.
        VideoTape => "video_tape",
        /// A memory card or stick, and the players sold as one.
        FlashMemory => "flash_memory",
        Floppy => "floppy",
        /// A phonograph cylinder.
        Cylinder => "cylinder",
        /// One disc with a CD side and a DVD side.
        DualDisc => "dual_disc",
        /// One disc with a vinyl side and a disc side.
        VinylDisc => "vinyl_disc",
        /// One disc with a CD layer and a DVD layer, glued rather than pressed
        /// as one piece.
        DvdPlus => "dvd_plus",
        /// A recordable CD with a lathe-cut groove on its label side.
        CdRecord => "cd_record",
        PianoRoll => "piano_roll",
        EdisonDisc => "edison_disc",
        PatheDisc => "pathe_disc",
        Tefifon => "tefifon",
        /// Universal Media Disc, the PlayStation Portable's disc.
        Umd => "umd",
        /// Video High Density, JVC's grooved video disc.
        Vhd => "vhd",
        /// RCA's grooved video disc, sold as SelectaVision.
        SelectaVision => "selecta_vision",
        /// Telefunken's video disc, sold as TeD.
        Ted => "ted",
        /// Music Video Disc.
        Mvd => "mvd",
        WireRecording => "wire_recording",
        FilmReel => "film_reel",
        MightyTiny => "mighty_tiny",
        Sopic => "sopic",
        /// A memory card sold as an album.
        KitAlbum => "kit_album",
        RomCartridge => "rom_cartridge",
        DataPlay => "data_play",
    }
}

impl Medium {
    /// The carrier a player pauses between the sides or discs of, for the
    /// carriers that have them: a grooved disc played one side at a time, a
    /// cassette turned over, a compact disc and its relatives changed.
    pub fn physical(self) -> Option<PhysicalMedium> {
        match self {
            Self::Vinyl | Self::Shellac | Self::Acetate | Self::EdisonDisc | Self::PatheDisc => {
                Some(PhysicalMedium::Record)
            }
            Self::Cassette | Self::Dcc => Some(PhysicalMedium::Cassette),
            Self::Cd | Self::Sacd | Self::Cdv | Self::VideoCd => Some(PhysicalMedium::Cd),
            _ => None,
        }
    }

    /// Whether the carrier is played side by side, so a track's position
    /// letter names its side.
    pub fn is_sided(self) -> bool {
        matches!(
            self.physical(),
            Some(PhysicalMedium::Record | PhysicalMedium::Cassette)
        )
    }
}

desktop_only! {
/// What a catalog's word for a medium turns out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lookup {
    /// A format name of the catalog's list, and the carrier it names.
    Carrier(Medium),
    /// A format name of the catalog's list that names no carrier:
    /// MusicBrainz's "Other" and "Phonograph record" say something about the
    /// medium without saying what it is made of.
    NamesNoCarrier,
    /// A word outside the catalog's list.
    Unrecognized,
}

/// What the name of a Discogs format entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiscogsFormatName {
    /// A medium, and the carrier it names.
    Carrier(Medium),
    /// A medium combining two basic formats, whose kind one of the entry's
    /// descriptions names.
    Hybrid,
    /// Not a medium: descriptions that apply to all of the release's media.
    AllMedia,
    /// Not a medium: the release's media are enclosed in a box.
    BoxSet,
}

impl Medium {
    /// What a MusicBrainz medium's format name names. A MusicBrainz medium
    /// states one format name and nothing else, so anything outside the list
    /// is a name the table has not been given yet.
    pub(crate) fn musicbrainz(name: &str) -> Lookup {
        found(MUSICBRAINZ_FORMAT_NAMES, name)
    }

    /// What the name of one Discogs format entry is. `None` for a name
    /// outside Discogs's list.
    pub(crate) fn discogs(name: &str) -> Option<DiscogsFormatName> {
        DISCOGS_FORMAT_NAMES
            .iter()
            .find(|(stated, _)| super::same_name(stated, name))
            .map(|(_, format)| *format)
    }
}

/// The entry of `table` whose name is `value`, whatever the case either is
/// written in — the catalogs are consistent about their own spelling but
/// nothing enforces it, and a lowercased "vinyl" names the same carrier.
fn found(table: &[(&str, Option<Medium>)], value: &str) -> Lookup {
    table
        .iter()
        .find(|(name, _)| super::same_name(name, value))
        .map_or(Lookup::Unrecognized, |(_, medium)| match medium {
            Some(medium) => Lookup::Carrier(*medium),
            None => Lookup::NamesNoCarrier,
        })
}

/// Every `medium_format` row of MusicBrainz, in the order the page lists
/// them. Source: https://musicbrainz.org/statistics/formats (generated from
/// the medium_format table), captured 2026-09-21. "Unknown format" — a NULL
/// format — is a medium with no name, which is an unstated entry rather than
/// a name to look up, so the list omits it.
const MUSICBRAINZ_FORMAT_NAMES: &[(&str, Option<Medium>)] = &[
    ("Digital Media", Some(Medium::Digital)),
    ("CD", Some(Medium::Cd)),
    ("12\" Vinyl", Some(Medium::Vinyl)),
    ("7\" Vinyl", Some(Medium::Vinyl)),
    ("Cassette", Some(Medium::Cassette)),
    ("CD-R", Some(Medium::Cd)),
    ("Vinyl", Some(Medium::Vinyl)),
    ("DVD-Video", Some(Medium::Dvd)),
    ("10\" Shellac", Some(Medium::Shellac)),
    ("Enhanced CD", Some(Medium::Cd)),
    ("8cm CD", Some(Medium::Cd)),
    ("10\" Vinyl", Some(Medium::Vinyl)),
    ("DVD", Some(Medium::Dvd)),
    ("Blu-ray", Some(Medium::BluRay)),
    ("Hybrid SACD", Some(Medium::Sacd)),
    ("SHM-CD", Some(Medium::Cd)),
    ("HDCD", Some(Medium::Cd)),
    ("Copy Control CD", Some(Medium::Cd)),
    ("Wax Cylinder", Some(Medium::Cylinder)),
    ("SACD", Some(Medium::Sacd)),
    ("Hybrid SACD (CD layer)", Some(Medium::Sacd)),
    ("Hybrid SACD (SACD layer, 2 channels)", Some(Medium::Sacd)),
    ("Data CD", Some(Medium::Cd)),
    ("Blu-spec CD", Some(Medium::Cd)),
    ("USB Flash Drive", Some(Medium::FlashMemory)),
    ("VHS", Some(Medium::VideoTape)),
    ("Download Card", Some(Medium::Digital)),
    ("HQCD", Some(Medium::Cd)),
    ("Phonograph record", None),
    ("Hybrid SACD (SACD layer, multichannel)", Some(Medium::Sacd)),
    ("Other", None),
    ("7\" Flexi-disc", Some(Medium::Vinyl)),
    ("DVD-Audio", Some(Medium::Dvd)),
    ("Mixed Mode CD", Some(Medium::Cd)),
    ("Shellac", Some(Medium::Shellac)),
    ("DAT", Some(Medium::Dat)),
    ("Reel-to-reel", Some(Medium::ReelToReel)),
    ("8-Track Cartridge", Some(Medium::Cartridge)),
    ("Hybrid SACD (SACD layer)", Some(Medium::Sacd)),
    ("VCD", Some(Medium::VideoCd)),
    ("12\" Shellac", Some(Medium::Shellac)),
    ("CD+G", Some(Medium::Cd)),
    ("MiniDisc", Some(Medium::MiniDisc)),
    ("Flexi-disc", Some(Medium::Vinyl)),
    ("SACD (2 channels)", Some(Medium::Sacd)),
    ("SHM-SACD", Some(Medium::Sacd)),
    ("DualDisc (CD side)", Some(Medium::DualDisc)),
    ("DVD-R Video", Some(Medium::Dvd)),
    ("DualDisc", Some(Medium::DualDisc)),
    ("DualDisc (DVD-Video side)", Some(Medium::DualDisc)),
    ("LaserDisc", Some(Medium::LaserDisc)),
    ("CDV", Some(Medium::Cdv)),
    ("Piano Roll", Some(Medium::PianoRoll)),
    ("Edison Diamond Disc", Some(Medium::EdisonDisc)),
    ("SHM-SACD (2 channels)", Some(Medium::Sacd)),
    ("DTS CD", Some(Medium::Cd)),
    ("10\" Acetate", Some(Medium::Acetate)),
    ("8cm CD+G", Some(Medium::Cd)),
    ("12\" LaserDisc", Some(Medium::LaserDisc)),
    ("7\" Shellac", Some(Medium::Shellac)),
    ("Data DVD", Some(Medium::Dvd)),
    ("12\" Acetate", Some(Medium::Acetate)),
    ("SACD (multichannel)", Some(Medium::Sacd)),
    ("3.5\" Floppy Disk", Some(Medium::Floppy)),
    ("DualDisc (DVD-Audio side)", Some(Medium::DualDisc)),
    ("KiT Album", Some(Medium::KitAlbum)),
    ("Cartridge", Some(Medium::Cartridge)),
    ("Data DVD-R", Some(Medium::Dvd)),
    ("VinylDisc", Some(Medium::VinylDisc)),
    ("Minimax CD", Some(Medium::Cd)),
    ("Microcassette", Some(Medium::Cassette)),
    ("8cm CD-R", Some(Medium::Cd)),
    ("Acetate", Some(Medium::Acetate)),
    ("VinylDisc (CD side)", Some(Medium::VinylDisc)),
    ("VinylDisc (Vinyl side)", Some(Medium::VinylDisc)),
    ("7\" Acetate", Some(Medium::Acetate)),
    ("3\" Vinyl", Some(Medium::Vinyl)),
    ("ROM cartridge", Some(Medium::RomCartridge)),
    ("Betacam SP", Some(Medium::VideoTape)),
    ("Playbutton", Some(Medium::FlashMemory)),
    ("Betamax", Some(Medium::VideoTape)),
    ("Blu-ray-R", Some(Medium::BluRay)),
    ("DCC", Some(Medium::Dcc)),
    ("Pathé disc", Some(Medium::PatheDisc)),
    ("SD Card", Some(Medium::FlashMemory)),
    ("Floppy Disk", Some(Medium::Floppy)),
    ("DualDisc (DVD side)", Some(Medium::DualDisc)),
    ("microSD", Some(Medium::FlashMemory)),
    ("DVDplus (DVD-Video side)", Some(Medium::DvdPlus)),
    ("DVDplus (CD side)", Some(Medium::DvdPlus)),
    ("UMD", Some(Medium::Umd)),
    ("CD-i", Some(Medium::Cd)),
    ("DataPlay", Some(Medium::DataPlay)),
    ("DVDplus", Some(Medium::DvdPlus)),
    ("VHD", Some(Medium::Vhd)),
    ("HD-DVD", Some(Medium::HdDvd)),
    ("PlayTape", Some(Medium::Cartridge)),
    ("slotMusic", Some(Medium::FlashMemory)),
    ("DVDplus (DVD-Audio side)", Some(Medium::DvdPlus)),
    ("SVCD", Some(Medium::VideoCd)),
    ("CED", Some(Medium::SelectaVision)),
    ("Zip Disk", Some(Medium::Floppy)),
    ("5.25\" Floppy Disk", Some(Medium::Floppy)),
    ("Ultra HD Blu-ray", Some(Medium::BluRay)),
    ("8\" LaserDisc", Some(Medium::LaserDisc)),
    ("MiniDVD", Some(Medium::Dvd)),
    ("Tefifon", Some(Medium::Tefifon)),
    ("Minimax DVD-Video", Some(Medium::Dvd)),
    ("SHM-SACD (multichannel)", Some(Medium::Sacd)),
    ("MiniDVD-Audio", Some(Medium::Dvd)),
    ("HiPac", Some(Medium::Cartridge)),
    ("MiniDVD-Video", Some(Medium::Dvd)),
    ("VinylDisc (DVD side)", Some(Medium::VinylDisc)),
    ("Minimax DVD", Some(Medium::Dvd)),
    ("Minimax DVD-Audio", Some(Medium::Dvd)),
];

/// The format names of Discogs, in the order the page lists them. Source:
/// https://www.discogs.com/help/formatslist, "The Format Field" table, read
/// through https://web.archive.org/web/20260103075003/https://www.discogs.com/help/formatslist,
/// captured 2026-01-03.
const DISCOGS_FORMAT_NAMES: &[(&str, DiscogsFormatName)] = &[
    ("Vinyl", DiscogsFormatName::Carrier(Medium::Vinyl)),
    ("Cassette", DiscogsFormatName::Carrier(Medium::Cassette)),
    ("CD", DiscogsFormatName::Carrier(Medium::Cd)),
    ("CDr", DiscogsFormatName::Carrier(Medium::Cd)),
    ("File", DiscogsFormatName::Carrier(Medium::Digital)),
    ("Acetate", DiscogsFormatName::Carrier(Medium::Acetate)),
    ("Flexi-disc", DiscogsFormatName::Carrier(Medium::Vinyl)),
    ("Lathe Cut", DiscogsFormatName::Carrier(Medium::Vinyl)),
    ("Shellac", DiscogsFormatName::Carrier(Medium::Shellac)),
    ("Mighty Tiny", DiscogsFormatName::Carrier(Medium::MightyTiny)),
    ("Sopic", DiscogsFormatName::Carrier(Medium::Sopic)),
    ("Pathé Disc", DiscogsFormatName::Carrier(Medium::PatheDisc)),
    ("Edison Disc", DiscogsFormatName::Carrier(Medium::EdisonDisc)),
    ("Cylinder", DiscogsFormatName::Carrier(Medium::Cylinder)),
    ("CDV", DiscogsFormatName::Carrier(Medium::Cdv)),
    ("DVD", DiscogsFormatName::Carrier(Medium::Dvd)),
    ("DVDr", DiscogsFormatName::Carrier(Medium::Dvd)),
    ("HD DVD", DiscogsFormatName::Carrier(Medium::HdDvd)),
    ("HD DVD-R", DiscogsFormatName::Carrier(Medium::HdDvd)),
    ("Blu-ray", DiscogsFormatName::Carrier(Medium::BluRay)),
    ("Blu-ray-R", DiscogsFormatName::Carrier(Medium::BluRay)),
    ("Ultra HD Blu-ray", DiscogsFormatName::Carrier(Medium::BluRay)),
    ("SACD", DiscogsFormatName::Carrier(Medium::Sacd)),
    ("4-Track Cartridge", DiscogsFormatName::Carrier(Medium::Cartridge)),
    ("8-Track Cartridge", DiscogsFormatName::Carrier(Medium::Cartridge)),
    ("DC-International", DiscogsFormatName::Carrier(Medium::Cassette)),
    ("Elcaset", DiscogsFormatName::Carrier(Medium::Cassette)),
    ("PlayTape", DiscogsFormatName::Carrier(Medium::Cartridge)),
    ("RCA Tape Cartridge", DiscogsFormatName::Carrier(Medium::Cartridge)),
    ("DAT", DiscogsFormatName::Carrier(Medium::Dat)),
    ("DCC", DiscogsFormatName::Carrier(Medium::Dcc)),
    ("Microcassette", DiscogsFormatName::Carrier(Medium::Cassette)),
    ("NT Cassette", DiscogsFormatName::Carrier(Medium::Cassette)),
    ("Pocket Rocker", DiscogsFormatName::Carrier(Medium::Cartridge)),
    ("Revere Magnetic Stereo Tape Ca", DiscogsFormatName::Carrier(Medium::Cartridge)),
    ("Tefifon", DiscogsFormatName::Carrier(Medium::Tefifon)),
    ("Reel-To-Reel", DiscogsFormatName::Carrier(Medium::ReelToReel)),
    ("Sabamobil", DiscogsFormatName::Carrier(Medium::Cartridge)),
    ("Beta ED", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("Betacam", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("Betacam SP", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("Betamax", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("Cartrivision", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("MiniDV", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("Super Beta", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("Super VHS", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("U-matic", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("VHS", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("Video 2000", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("Video8", DiscogsFormatName::Carrier(Medium::VideoTape)),
    ("Film Reel", DiscogsFormatName::Carrier(Medium::FilmReel)),
    ("HitClips", DiscogsFormatName::Carrier(Medium::FlashMemory)),
    ("Laserdisc", DiscogsFormatName::Carrier(Medium::LaserDisc)),
    ("SelectaVision", DiscogsFormatName::Carrier(Medium::SelectaVision)),
    ("TeD", DiscogsFormatName::Carrier(Medium::Ted)),
    ("VHD", DiscogsFormatName::Carrier(Medium::Vhd)),
    ("Wire Recording", DiscogsFormatName::Carrier(Medium::WireRecording)),
    ("Minidisc", DiscogsFormatName::Carrier(Medium::MiniDisc)),
    ("MVD", DiscogsFormatName::Carrier(Medium::Mvd)),
    ("UMD", DiscogsFormatName::Carrier(Medium::Umd)),
    ("Floppy Disk", DiscogsFormatName::Carrier(Medium::Floppy)),
    ("Zip Disk", DiscogsFormatName::Carrier(Medium::Floppy)),
    ("Memory Stick", DiscogsFormatName::Carrier(Medium::FlashMemory)),
    ("Hybrid", DiscogsFormatName::Hybrid),
    ("All Media", DiscogsFormatName::AllMedia),
    ("Box Set", DiscogsFormatName::BoxSet),
];

}

#[cfg(test)]
mod tests {
    use super::super::recorded;
    use super::*;

    /// The tables are the catalogs' lists, name for name and in their order.
    /// The captured pages sit beside the crate, so the code and the record
    /// are compared wherever the tests run.
    #[test]
    fn each_table_is_the_catalogs_own_list() {
        assert_eq!(
            MUSICBRAINZ_FORMAT_NAMES
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            recorded(include_str!(
                "../../test-fixtures/pressing-vocabulary/musicbrainz-format-names.txt"
            ))
        );
        assert_eq!(
            DISCOGS_FORMAT_NAMES
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            recorded(include_str!(
                "../../test-fixtures/pressing-vocabulary/discogs-format-names.txt"
            ))
        );
    }

    /// Each catalog's word is read in that catalog's own vocabulary, and
    /// every way a word can land has its answer.
    #[test]
    fn a_word_lands_as_a_carrier_a_name_or_nothing() {
        assert_eq!(
            Medium::musicbrainz("Hybrid SACD (SACD layer)"),
            Lookup::Carrier(Medium::Sacd)
        );
        assert_eq!(Medium::musicbrainz("Other"), Lookup::NamesNoCarrier);
        assert_eq!(
            Medium::musicbrainz("File"),
            Lookup::Unrecognized,
            "a Discogs format name is not a MusicBrainz one"
        );
        assert_eq!(
            Medium::discogs("File"),
            Some(DiscogsFormatName::Carrier(Medium::Digital))
        );
        assert_eq!(Medium::discogs("Box Set"), Some(DiscogsFormatName::BoxSet));
        assert_eq!(
            Medium::discogs("FLAC"),
            None,
            "a description is not a format name"
        );
    }

    /// A name matches as a whole word, not as a part of one: the catalogs
    /// state a format name, and a string that merely carries one is a
    /// description of something else.
    #[test]
    fn a_name_matches_whole_and_in_any_case() {
        assert_eq!(
            Medium::discogs("vinyl"),
            Some(DiscogsFormatName::Carrier(Medium::Vinyl))
        );
        assert_eq!(Medium::musicbrainz("8CM cd+g"), Lookup::Carrier(Medium::Cd));
        assert_eq!(
            Medium::discogs("PATHÉ DISC"),
            Some(DiscogsFormatName::Carrier(Medium::PatheDisc))
        );
        assert_eq!(Medium::musicbrainz("2xCD"), Lookup::Unrecognized);
        assert_eq!(Medium::discogs("CD Album"), None);
    }

    /// Only the carriers played side by side read a position's letter as a
    /// side.
    #[test]
    fn records_and_cassettes_are_sided() {
        assert!(Medium::Shellac.is_sided());
        assert!(Medium::Cassette.is_sided());
        assert!(!Medium::Cd.is_sided());
        assert!(!Medium::Digital.is_sided());
        assert_eq!(Medium::Sacd.physical(), Some(PhysicalMedium::Cd));
    }
}
