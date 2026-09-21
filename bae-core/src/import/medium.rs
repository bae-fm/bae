//! The carriers the two catalogs name, in their own words.
//!
//! MusicBrainz and Discogs each state a release's media from a closed list:
//! MusicBrainz names one format per medium, Discogs names the release's
//! format names followed by its descriptions in one flat array. Both lists
//! are reproduced below, name for name, and each name is given the carrier
//! family a person would call a different object — a CD-R and an SHM-CD are
//! both a CD, a File is a download, and a cassette is neither.
//!
//! The lists are the vocabulary: a name outside them is not a medium this
//! module knows, and the test that compares each table against its captured
//! record is what keeps the code and the record from drifting apart.

/// A carrier family: what the physical (or downloaded) object a pressing is
/// made of is, as far as two records of it can be told apart. One variant
/// per family a person would call a different object; the sizes, speeds,
/// pressing plants and layer counts the catalogs also name are the same
/// object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Medium {
    Cd,
    Sacd,
    Dvd,
    HdDvd,
    BluRay,
    Vinyl,
    Shellac,
    Acetate,
    Cassette,
    /// Digital Compact Cassette.
    Dcc,
    /// Digital Audio Tape.
    Dat,
    /// A tape cartridge: the 4- and 8-track families and the players that
    /// took their own cartridge.
    Cartridge,
    ReelToReel,
    /// A download or a card that carries one, which is a copy rather than an
    /// object.
    Digital,
    MiniDisc,
    LaserDisc,
    /// A CD carrying an analogue video track.
    Cdv,
    /// Video CD and its super form.
    VideoCd,
    /// A video cassette, whichever of the consumer and broadcast families.
    VideoTape,
    /// A memory card or stick, and the players sold as one.
    FlashMemory,
    Floppy,
    /// A phonograph cylinder.
    Cylinder,
    /// One disc with a CD side and a DVD side.
    DualDisc,
    /// One disc with a vinyl side and a disc side.
    VinylDisc,
    /// One disc with a CD layer and a DVD layer, glued rather than pressed
    /// as one piece.
    DvdPlus,
    PianoRoll,
    EdisonDisc,
    PatheDisc,
    Tefifon,
    /// Universal Media Disc, the PlayStation Portable's disc.
    Umd,
    /// Video High Density, JVC's grooved video disc.
    Vhd,
    /// RCA's grooved video disc, sold as SelectaVision.
    SelectaVision,
    /// Telefunken's video disc, sold as TeD.
    Ted,
    /// Music Video Disc.
    Mvd,
    WireRecording,
    FilmReel,
    MightyTiny,
    Sopic,
    /// A memory card sold as an album.
    KitAlbum,
    RomCartridge,
    DataPlay,
}

/// What a catalog's word for a medium turns out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lookup {
    /// A format name of the catalog's list, and the carrier it names.
    Carrier(Medium),
    /// A format name of the catalog's list that names no carrier:
    /// MusicBrainz's "Other" and "Phonograph record", and Discogs's
    /// "Hybrid", "All Media" and "Box Set", say something about the release
    /// without saying what it is made of.
    NamesNoCarrier,
    /// A Discogs description: what is on the carrier, how it was cut, or how
    /// it was sold — never the carrier itself.
    Description,
    /// A word in neither of the catalog's lists.
    Unrecognized,
}

impl Medium {
    /// What a MusicBrainz medium's format name names. A MusicBrainz medium
    /// states one format name and nothing else, so anything outside the list
    /// is a name the table has not been given yet.
    pub(crate) fn musicbrainz(name: &str) -> Lookup {
        found(MUSICBRAINZ_FORMAT_NAMES, name).unwrap_or(Lookup::Unrecognized)
    }

    /// What one token of a Discogs `format` array is. The array is the
    /// release's format names followed by their descriptions, so a token is
    /// read against the names first and the descriptions second; "Hybrid",
    /// which both lists carry, names no carrier either way.
    pub(crate) fn discogs(token: &str) -> Lookup {
        found(DISCOGS_FORMAT_NAMES, token)
            .or_else(|| {
                DISCOGS_FORMAT_DESCRIPTIONS
                    .iter()
                    .any(|description| same_name(description, token))
                    .then_some(Lookup::Description)
            })
            .unwrap_or(Lookup::Unrecognized)
    }
}

/// The entry of `table` whose name is `value`, whatever the case either is
/// written in — the catalogs are consistent about their own spelling but
/// nothing enforces it, and a lowercased "vinyl" names the same carrier.
fn found(table: &[(&str, Option<Medium>)], value: &str) -> Option<Lookup> {
    table
        .iter()
        .find(|(name, _)| same_name(name, value))
        .map(|(_, medium)| match medium {
            Some(medium) => Lookup::Carrier(*medium),
            None => Lookup::NamesNoCarrier,
        })
}

/// Whether two names are the same word. Compared character by character in
/// lower case, so neither side is allocated and the accented names compare
/// the way the unaccented ones do.
fn same_name(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
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
const DISCOGS_FORMAT_NAMES: &[(&str, Option<Medium>)] = &[
    ("Vinyl", Some(Medium::Vinyl)),
    ("Cassette", Some(Medium::Cassette)),
    ("CD", Some(Medium::Cd)),
    ("CDr", Some(Medium::Cd)),
    ("File", Some(Medium::Digital)),
    ("Acetate", Some(Medium::Acetate)),
    ("Flexi-disc", Some(Medium::Vinyl)),
    ("Lathe Cut", Some(Medium::Vinyl)),
    ("Shellac", Some(Medium::Shellac)),
    ("Mighty Tiny", Some(Medium::MightyTiny)),
    ("Sopic", Some(Medium::Sopic)),
    ("Pathé Disc", Some(Medium::PatheDisc)),
    ("Edison Disc", Some(Medium::EdisonDisc)),
    ("Cylinder", Some(Medium::Cylinder)),
    ("CDV", Some(Medium::Cdv)),
    ("DVD", Some(Medium::Dvd)),
    ("DVDr", Some(Medium::Dvd)),
    ("HD DVD", Some(Medium::HdDvd)),
    ("HD DVD-R", Some(Medium::HdDvd)),
    ("Blu-ray", Some(Medium::BluRay)),
    ("Blu-ray-R", Some(Medium::BluRay)),
    ("Ultra HD Blu-ray", Some(Medium::BluRay)),
    ("SACD", Some(Medium::Sacd)),
    ("4-Track Cartridge", Some(Medium::Cartridge)),
    ("8-Track Cartridge", Some(Medium::Cartridge)),
    ("DC-International", Some(Medium::Cassette)),
    ("Elcaset", Some(Medium::Cassette)),
    ("PlayTape", Some(Medium::Cartridge)),
    ("RCA Tape Cartridge", Some(Medium::Cartridge)),
    ("DAT", Some(Medium::Dat)),
    ("DCC", Some(Medium::Dcc)),
    ("Microcassette", Some(Medium::Cassette)),
    ("NT Cassette", Some(Medium::Cassette)),
    ("Pocket Rocker", Some(Medium::Cartridge)),
    ("Revere Magnetic Stereo Tape Ca", Some(Medium::Cartridge)),
    ("Tefifon", Some(Medium::Tefifon)),
    ("Reel-To-Reel", Some(Medium::ReelToReel)),
    ("Sabamobil", Some(Medium::Cartridge)),
    ("Beta ED", Some(Medium::VideoTape)),
    ("Betacam", Some(Medium::VideoTape)),
    ("Betacam SP", Some(Medium::VideoTape)),
    ("Betamax", Some(Medium::VideoTape)),
    ("Cartrivision", Some(Medium::VideoTape)),
    ("MiniDV", Some(Medium::VideoTape)),
    ("Super Beta", Some(Medium::VideoTape)),
    ("Super VHS", Some(Medium::VideoTape)),
    ("U-matic", Some(Medium::VideoTape)),
    ("VHS", Some(Medium::VideoTape)),
    ("Video 2000", Some(Medium::VideoTape)),
    ("Video8", Some(Medium::VideoTape)),
    ("Film Reel", Some(Medium::FilmReel)),
    ("HitClips", Some(Medium::FlashMemory)),
    ("Laserdisc", Some(Medium::LaserDisc)),
    ("SelectaVision", Some(Medium::SelectaVision)),
    ("TeD", Some(Medium::Ted)),
    ("VHD", Some(Medium::Vhd)),
    ("Wire Recording", Some(Medium::WireRecording)),
    ("Minidisc", Some(Medium::MiniDisc)),
    ("MVD", Some(Medium::Mvd)),
    ("UMD", Some(Medium::Umd)),
    ("Floppy Disk", Some(Medium::Floppy)),
    ("Zip Disk", Some(Medium::Floppy)),
    ("Memory Stick", Some(Medium::FlashMemory)),
    ("Hybrid", None),
    ("All Media", None),
    ("Box Set", None),
];

/// The descriptions of Discogs, from the same page's "The Description Field"
/// table, in page order with the repeats dropped — the page lists a
/// description again under each format it applies to. A description says
/// what is on a carrier, never which carrier, so none of them names one.
const DISCOGS_FORMAT_DESCRIPTIONS: &[&str] = &[
    "LP",
    "16\"",
    "12\"",
    "14\"",
    "11\"",
    "10\"",
    "9\"",
    "8\"",
    "7\"",
    "6½\"",
    "6\"",
    "5½\"",
    "5\"",
    "4\"",
    "3½\"",
    "3\"",
    "2\"",
    "1\"",
    "8 ⅓ RPM",
    "16 ⅔ RPM",
    "33 ⅓ RPM",
    "45 RPM",
    "78 RPM",
    "120 RPM",
    "21cm",
    "25cm",
    "27cm",
    "29cm",
    "35cm",
    "40cm",
    "50cm",
    "80 RPM",
    "90 RPM",
    "15/16 ips",
    "1 ⅞ ips",
    "15 ips",
    "3 ¾ ips",
    "30 ips",
    "7 ½ ips",
    "½\"",
    "¼\"",
    "⅛\"",
    "2-Track Mono",
    "2-Track Stereo",
    "4-Track Mono",
    "4-Track Stereo",
    "10.5\" NAB Reel",
    "3\" Cine Reel",
    "5\" Cine Reel",
    "6\" Cine Reel",
    "7\" Cine Reel",
    "2 Minute",
    "3 Minute",
    "4 Minute",
    "Concert",
    "Salon",
    "Mini",
    "Business Card",
    "Shape",
    "Minimax",
    "CD-ROM",
    "CDi",
    "CD+G",
    "HDCD",
    "VCD",
    "AVCD",
    "SVCD",
    "XRCD",
    "4K",
    "8K",
    "Blu-ray Audio",
    "Multichannel",
    "DVD-Audio",
    "DVD-Data",
    "DVD-Video",
    "Hybrid",
    "AAC",
    "AIFC",
    "AIFF",
    "ALAC",
    "AMR",
    "APE",
    "AVI",
    "DFF",
    "Disc Image",
    "DSF",
    "FLAC",
    "FLV",
    "MOV",
    "MP1",
    "MP2",
    "MP3",
    "MPEG Video",
    "MPEG-4 Video",
    "ogg-vorbis",
    "Opus",
    "RA",
    "RM",
    "SHN",
    "SPX",
    "SWF",
    "TTA",
    "WAV",
    "WavPack",
    "WMA",
    "WMV",
    "MP3 Surround",
    "3.5\"",
    "5.25\"",
    "CD-Record",
    "DualDisc",
    "DVDplus",
    "VinylDisc",
    "Double Sided",
    "Single Sided",
    "Advance",
    "Album",
    "Mini-Album",
    "EP",
    "Maxi-Single",
    "Record Store Day",
    "Single",
    "Compilation",
    "Stereo",
    "Mono",
    "Quadraphonic",
    "Ambisonic",
    "Bioplastic",
    "Card Backed",
    "Club Edition",
    "Copy Protected",
    "Deluxe Edition",
    "Enhanced",
    "Etched",
    "Jukebox",
    "Limited Edition",
    "Mispress",
    "Misprint",
    "Mixed",
    "Mixtape",
    "Numbered",
    "Partially Mixed",
    "Partially Unofficial",
    "Picture Disc",
    "Promo",
    "Reissue",
    "Remastered",
    "Repress",
    "Sampler",
    "Special Cut",
    "Special Edition",
    "Styrene",
    "Test Pressing",
    "Tour Recording",
    "Transcription",
    "Unofficial Release",
    "White Label",
    "16mm",
    "35mm",
    "NTSC",
    "PAL",
    "SECAM",

];

#[cfg(test)]
mod tests {
    use super::*;

    /// The names of a captured page: one per line, with the line naming the
    /// source left out.
    fn recorded(page: &str) -> Vec<&str> {
        page.lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect()
    }

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
                "../../test-fixtures/medium-vocabulary/musicbrainz-format-names.txt"
            ))
        );
        assert_eq!(
            DISCOGS_FORMAT_NAMES
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            recorded(include_str!(
                "../../test-fixtures/medium-vocabulary/discogs-format-names.txt"
            ))
        );
        assert_eq!(
            DISCOGS_FORMAT_DESCRIPTIONS.to_vec(),
            recorded(include_str!(
                "../../test-fixtures/medium-vocabulary/discogs-format-descriptions.txt"
            ))
        );
    }

    /// Each catalog's word is read in that catalog's own vocabulary, and
    /// every way a word can land has its answer.
    #[test]
    fn a_word_lands_as_a_carrier_a_name_a_description_or_nothing() {
        assert_eq!(
            Medium::musicbrainz("Hybrid SACD (SACD layer)"),
            Lookup::Carrier(Medium::Sacd)
        );
        assert_eq!(Medium::musicbrainz("Other"), Lookup::NamesNoCarrier);
        assert_eq!(
            Medium::musicbrainz("FLAC"),
            Lookup::Unrecognized,
            "a Discogs description is not a MusicBrainz format name"
        );
        assert_eq!(Medium::discogs("File"), Lookup::Carrier(Medium::Digital));
        assert_eq!(Medium::discogs("Box Set"), Lookup::NamesNoCarrier);
        assert_eq!(Medium::discogs("FLAC"), Lookup::Description);
        assert_eq!(Medium::discogs("Zorblax"), Lookup::Unrecognized);
    }

    /// A name matches as a whole word, not as a part of one: the catalogs
    /// state a format name, and a string that merely carries one is a
    /// description of something else.
    #[test]
    fn a_name_matches_whole_and_in_any_case() {
        assert_eq!(Medium::discogs("vinyl"), Lookup::Carrier(Medium::Vinyl));
        assert_eq!(Medium::musicbrainz("8CM cd+g"), Lookup::Carrier(Medium::Cd));
        assert_eq!(
            Medium::discogs("PATHÉ DISC"),
            Lookup::Carrier(Medium::PatheDisc)
        );
        assert_eq!(Medium::musicbrainz("2xCD"), Lookup::Unrecognized);
        assert_eq!(Medium::discogs("CD Album"), Lookup::Unrecognized);
    }
}
