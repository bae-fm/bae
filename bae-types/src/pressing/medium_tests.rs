use super::*;
use crate::pressing::recorded;

/// The tables are the catalogs' lists, name for name and in their order.
/// The captured pages sit beside the crate, so the code and the record
/// are compared wherever the tests run.
#[test]
fn each_table_is_the_catalogs_own_list() {
    assert_eq!(
        MUSICBRAINZ_FORMAT_NAMES.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        recorded(include_str!(
            "../../test-fixtures/pressing-vocabulary/musicbrainz-format-names.txt"
        ))
    );
    assert_eq!(
        DISCOGS_FORMAT_NAMES.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        recorded(include_str!("../../test-fixtures/pressing-vocabulary/discogs-format-names.txt"))
    );
}

/// Each catalog's word is read in that catalog's own vocabulary, and
/// every way a word can land has its answer.
#[test]
fn a_word_lands_as_a_carrier_a_name_or_nothing() {
    assert_eq!(Medium::musicbrainz("Hybrid SACD (SACD layer)"), Lookup::Carrier(Medium::Sacd));
    assert_eq!(Medium::musicbrainz("Other"), Lookup::NamesNoCarrier);
    assert_eq!(
        Medium::musicbrainz("File"),
        Lookup::Unrecognized,
        "a Discogs format name is not a MusicBrainz one"
    );
    assert_eq!(Medium::discogs("File"), Some(DiscogsFormatName::Carrier(Medium::Digital)));
    assert_eq!(Medium::discogs("Box Set"), Some(DiscogsFormatName::BoxSet));
    assert_eq!(Medium::discogs("FLAC"), None, "a description is not a format name");
}

/// A name matches as a whole word, not as a part of one: the catalogs
/// state a format name, and a string that merely carries one is a
/// description of something else.
#[test]
fn a_name_matches_whole_and_in_any_case() {
    assert_eq!(Medium::discogs("vinyl"), Some(DiscogsFormatName::Carrier(Medium::Vinyl)));
    assert_eq!(Medium::musicbrainz("8CM cd+g"), Lookup::Carrier(Medium::Cd));
    assert_eq!(Medium::discogs("PATHÉ DISC"), Some(DiscogsFormatName::Carrier(Medium::PatheDisc)));
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

/// A disc with a CD side plays CD audio among the rest, and an SACD may
/// or may not carry a CD layer, so neither is ruled in or out by a
/// folder's audio.
#[test]
fn a_carrier_plays_cd_audio_only_partly_or_never() {
    assert_eq!(Medium::Cd.cd_audio(), CdAudio::Only);
    assert_eq!(Medium::DualDisc.cd_audio(), CdAudio::Partly);
    assert_eq!(Medium::Sacd.cd_audio(), CdAudio::Partly);
    assert_eq!(Medium::VideoCd.cd_audio(), CdAudio::Never);
    assert_eq!(Medium::Vinyl.cd_audio(), CdAudio::Never);
    assert_eq!(Medium::Digital.cd_audio(), CdAudio::Never);
}
