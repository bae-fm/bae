use super::*;

#[test]
fn the_list_is_musicbrainz_s_list() {
    let all: Vec<_> = Instrument::all().collect();
    assert_eq!(all.len(), 1062);
    assert_eq!(all.len(), INSTRUMENT_COUNT);
    assert_eq!(all[0].name(), "12 string guitar");
    let ids: std::collections::HashSet<_> = all.iter().map(|instrument| instrument.id()).collect();
    assert_eq!(ids.len(), all.len(), "an id names one instrument");
}

#[test]
fn an_instrument_is_found_by_its_id_and_by_its_name() {
    let bass = Instrument::from_id("17f9f065-2312-4a24-8309-6f6dd63e2e33").unwrap();
    assert_eq!(bass.name(), "bass guitar");
    assert_eq!(Instrument::from_name("bass guitar"), Some(bass));
    assert!(Instrument::from_id("00000000-0000-4000-8000-000000000000").is_none());
    assert!(Instrument::from_name("kazoo orchestra of the moon").is_none());
}

/// Every line of the list is an id and a name with no stray whitespace.
#[test]
fn every_line_of_the_list_is_an_id_and_a_name() {
    for line in
        INSTRUMENT_LIST.lines().chain(VOICE_LIST.lines()).filter(|line| !line.starts_with('#'))
    {
        let (id, name) = line.split_once('\t').expect("an id and a name");
        assert_eq!(id.len(), 36, "{line}");
        assert!(!name.is_empty() && name == name.trim() && !name.contains('\t'), "{line}");
    }
}

#[test]
fn the_voices_are_the_vocal_attribute_and_its_values() {
    let all: Vec<_> = Voice::all().collect();
    assert_eq!(all.len(), 18);
    assert_eq!(Voice::general().name(), "vocal");
    assert_eq!(
        Voice::from_name("lead vocals").unwrap().id(),
        "8e2a3255-87c2-4809-a174-98cb3704f1a5"
    );
    assert_eq!(
        Voice::from_id("75052401-7340-4e5b-a71d-ea024a128849").unwrap().name(),
        "background vocals"
    );
    assert!(Voice::from_name("falsetto vocals").is_none());
    assert!(Voice::from_id("00000000-0000-4000-8000-000000000000").is_none());
    for voice in all {
        assert_eq!(Voice::from_id(voice.id()), Some(voice));
    }
}

/// The lists are read from their captured text at compile time: the entries
/// are the text's lines.
#[test]
fn a_list_is_read_from_its_text() {
    const TEXT: &str = "# header\nid-1\tfirst name\n\nid-2\tsecond\n";
    const COUNT: usize = table::count(TEXT);
    const ENTRIES: table::Entries<COUNT> = table::parse::<COUNT>(TEXT);
    assert_eq!(ENTRIES, [("id-1", "first name"), ("id-2", "second")]);
}
