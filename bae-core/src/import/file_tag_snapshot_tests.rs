use super::*;

const PLACEHOLDER_MP3: &[u8] =
    include_bytes!("../../test-fixtures/audio-format/placeholder-mp3.mp3");

fn synchsafe(value: usize) -> [u8; 4] {
    [
        ((value >> 21) & 0x7f) as u8,
        ((value >> 14) & 0x7f) as u8,
        ((value >> 7) & 0x7f) as u8,
        (value & 0x7f) as u8,
    ]
}

fn legacy_id3v23_frame(id: &[u8; 4], value: &str) -> Vec<u8> {
    let (encoded, _, had_errors) = encoding_rs::WINDOWS_1251.encode(value);
    assert!(!had_errors);
    let size = encoded.len() + 1;
    let mut frame = Vec::with_capacity(10 + size);
    frame.extend_from_slice(id);
    frame.extend_from_slice(&(size as u32).to_be_bytes());
    frame.extend_from_slice(&[0, 0]);
    frame.push(0);
    frame.extend_from_slice(&encoded);
    frame
}

fn legacy_cyrillic_mp3() -> Vec<u8> {
    let mut frames = Vec::new();
    frames.extend(legacy_id3v23_frame(b"TIT2", "Название дорожки"));
    frames.extend(legacy_id3v23_frame(b"TPE1", "Имя исполнителя"));
    frames.extend(legacy_id3v23_frame(b"TALB", "Название альбома"));
    frames.extend(legacy_id3v23_frame(b"TPE2", "Исполнитель альбома"));

    let mut file = b"ID3\x03\x00\x00".to_vec();
    file.extend_from_slice(&synchsafe(frames.len()));
    file.extend(frames);

    let existing_tag_size = 10
        + PLACEHOLDER_MP3[6..10]
            .iter()
            .fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
    file.extend_from_slice(&PLACEHOLDER_MP3[existing_tag_size..]);
    file
}

fn legacy_cyrillic_multivalue_mp3() -> Vec<u8> {
    let mut frames = legacy_id3v23_frame(b"TPE1", "Исполнитель Один");
    let separator = frames.len();
    frames.extend(legacy_id3v23_frame(b"TPE1", "Исполнитель Два"));
    let second_frame = frames.split_off(separator);
    let second_value = &second_frame[11..];
    let combined_size = frames.len() - 10 + 1 + second_value.len();
    frames[4..8].copy_from_slice(&(combined_size as u32).to_be_bytes());
    frames.push(0);
    frames.extend_from_slice(second_value);

    let mut file = b"ID3\x04\x00\x00".to_vec();
    file.extend_from_slice(&synchsafe(frames.len()));
    file.extend(frames);
    let existing_tag_size = 10
        + PLACEHOLDER_MP3[6..10]
            .iter()
            .fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
    file.extend_from_slice(&PLACEHOLDER_MP3[existing_tag_size..]);
    file
}

fn legacy_cyrillic_id3v1_mp3() -> Vec<u8> {
    let existing_tag_size = 10
        + PLACEHOLDER_MP3[6..10]
            .iter()
            .fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
    let mut file = PLACEHOLDER_MP3[existing_tag_size..].to_vec();
    let mut tag = [0_u8; 128];
    tag[..3].copy_from_slice(b"TAG");
    write_id3v1_field(&mut tag[3..33], "Название дорожки");
    write_id3v1_field(&mut tag[33..63], "Имя исполнителя");
    write_id3v1_field(&mut tag[63..93], "Название альбома");
    tag[127] = u8::MAX;
    file.extend_from_slice(&tag);
    file
}

fn write_id3v1_field(destination: &mut [u8], value: &str) {
    let (encoded, _, had_errors) = encoding_rs::WINDOWS_1251.encode(value);
    assert!(!had_errors);
    assert!(encoded.len() <= destination.len());
    destination[..encoded.len()].copy_from_slice(&encoded);
}

/// Placeholder JPEG bytes: a start-of-image marker, an APP0 marker and
/// a scan marker whose `0xff` bytes each need an unsynchronisation guard
/// byte, and an end-of-image marker.
const PLACEHOLDER_JPEG: &[u8] = &[
    0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0xff, 0x00, 0xff, 0xda, 0x12,
    0x34, 0xff, 0xd9,
];

/// ID3v2 unsynchronisation: a `0x00` after every `0xff` that precedes a
/// byte of `0xe0` or more, or a `0x00`.
fn unsynchronise(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 2);
    for (index, byte) in data.iter().enumerate() {
        out.push(*byte);
        let next = data.get(index + 1).copied();
        if *byte == 0xff && next.is_some_and(|next| next >= 0xe0 || next == 0x00) {
            out.push(0x00);
        }
    }
    out
}

/// An ID3v2.4 tag the way some taggers write one: the header's
/// unsynchronisation flag set, and each frame unsynchronised with a data
/// length indicator (frame flags `0x0003`). A title frame follows the
/// picture so a misread picture size shows up as a lost title.
fn unsynchronised_id3v24_mp3(jpeg: &[u8]) -> Vec<u8> {
    fn frame(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let stored = unsynchronise(data);
        let mut frame = Vec::new();
        frame.extend_from_slice(id);
        frame.extend_from_slice(&synchsafe(4 + stored.len()));
        frame.extend_from_slice(&[0x00, 0x03]);
        frame.extend_from_slice(&synchsafe(data.len()));
        frame.extend(stored);
        frame
    }
    let mut picture = vec![0x00];
    picture.extend_from_slice(b"image/jpeg\0");
    picture.push(0x03);
    picture.push(0x00);
    picture.extend_from_slice(jpeg);
    let mut title = vec![0x03];
    title.extend_from_slice("Track Alpha".as_bytes());

    let mut frames = frame(b"APIC", &picture);
    frames.extend(frame(b"TIT2", &title));

    let mut file = b"ID3\x04\x00\x80".to_vec();
    file.extend_from_slice(&synchsafe(frames.len()));
    file.extend(frames);
    let existing_tag_size = 10
        + PLACEHOLDER_MP3[6..10]
            .iter()
            .fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
    file.extend_from_slice(&PLACEHOLDER_MP3[existing_tag_size..]);
    file
}

fn snapshot(embedded_cover: Option<EmbeddedCoverFact>) -> FileTagSnapshot {
    FileTagSnapshot {
        scan_generation: 1,
        file_edit_revision: 0,
        files: Vec::new(),
        embedded_cover,
    }
}

#[derive(Default)]
struct CountingReader(std::sync::atomic::AtomicUsize);

impl FileTagReader for CountingReader {
    fn read(&self, _path: &Path) -> Result<FileTagRead, ImportError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(FileTagRead {
            tags: FileTags {
                title: Some("Track Alpha".to_string()),
                track_artists: vec!["Artist Alpha".to_string()],
                album_title: Some("Album Alpha".to_string()),
                album_artists: vec!["Artist Alpha".to_string()],
                years: vec![2001],
                numbers: crate::import::track_numbers::TrackNumbers {
                    disc: None,
                    track: Some(1),
                },
                ..FileTags::untagged()
            },
            embedded_cover: None,
        })
    }
}

/// A track's ISRC is read off its tag as the tag writes it.
#[test]
fn a_tag_s_isrc_is_read() {
    let mut frames = legacy_id3v23_frame(b"TIT2", "Track Alpha");
    frames.extend(legacy_id3v23_frame(b"TSRC", "IT-00G-91-70501"));
    let mut file = b"ID3\x03\x00\x00".to_vec();
    file.extend_from_slice(&synchsafe(frames.len()));
    file.extend(frames);
    let existing_tag_size = 10
        + PLACEHOLDER_MP3[6..10]
            .iter()
            .fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
    file.extend_from_slice(&PLACEHOLDER_MP3[existing_tag_size..]);
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("01.mp3");
    std::fs::write(&path, file).unwrap();

    assert_eq!(
        LoftyFileTagReader.read(&path).unwrap().tags.isrcs,
        ["IT-00G-91-70501"]
    );
}

#[test]
fn embedded_cover_in_an_unsynchronised_id3v24_tag_reads_as_the_original_image() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("01.mp3");
    std::fs::write(&path, unsynchronised_id3v24_mp3(PLACEHOLDER_JPEG)).unwrap();

    let (bytes, content_type) = LoftyFileTagReader
        .read(&path)
        .unwrap()
        .embedded_cover
        .expect("the tag carries a picture");

    assert_eq!(content_type, ContentType::Jpeg);
    assert_eq!(bytes, PLACEHOLDER_JPEG);
    assert_eq!(
        LoftyFileTagReader
            .read(&path)
            .unwrap()
            .tags
            .title
            .as_deref(),
        Some("Track Alpha")
    );
}

#[test]
fn lofty_reader_decodes_legacy_cyrillic_id3_text() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("01.mp3");
    std::fs::write(&path, legacy_cyrillic_mp3()).unwrap();

    let read = LoftyFileTagReader.read(&path).unwrap().tags;

    assert_eq!(read.title.as_deref(), Some("Название дорожки"));
    assert_eq!(read.track_artists, ["Имя исполнителя"]);
    assert_eq!(read.album_title.as_deref(), Some("Название альбома"));
    assert_eq!(read.album_artists, ["Исполнитель альбома"]);
}

/// An ID3v2.4 frame of two values states both, each decoded.
#[test]
fn lofty_reader_keeps_each_value_of_an_id3v24_multivalue_frame() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("01.mp3");
    std::fs::write(&path, legacy_cyrillic_multivalue_mp3()).unwrap();

    let read = LoftyFileTagReader.read(&path).unwrap().tags;

    assert_eq!(read.track_artists, ["Исполнитель Один", "Исполнитель Два"]);
}

#[test]
fn lofty_reader_decodes_legacy_cyrillic_id3v1_text() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("01.mp3");
    std::fs::write(&path, legacy_cyrillic_id3v1_mp3()).unwrap();

    let read = LoftyFileTagReader.read(&path).unwrap().tags;

    assert_eq!(read.title.as_deref(), Some("Название дорожки"));
    assert_eq!(read.track_artists, ["Имя исполнителя"]);
    assert_eq!(read.album_title.as_deref(), Some("Название альбома"));
}

#[test]
fn extraction_reads_every_file_before_returning_one_snapshot() {
    let dir = tempfile::TempDir::new().unwrap();
    let paths = [dir.path().join("01.flac"), dir.path().join("02.flac")];
    for path in &paths {
        std::fs::write(path, b"audio").unwrap();
    }
    let files = paths
        .iter()
        .enumerate()
        .map(|(index, path)| ScannedFile::new(path.clone(), format!("{:02}.flac", index + 1), 5, 1))
        .collect::<Vec<_>>();
    let reader = CountingReader::default();

    let snapshot = extract_file_tag_snapshot(&files, 8, 2, &reader).unwrap();

    assert_eq!(snapshot.files.len(), 2);
    assert_eq!(reader.0.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert_eq!(
        snapshot
            .files
            .iter()
            .map(|fact| &fact.observation)
            .collect::<Vec<_>>(),
        observe_audio_files(&files)
            .unwrap()
            .iter()
            .collect::<Vec<_>>()
    );
}

#[test]
fn extraction_refuses_a_file_whose_scanned_size_changed() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("01.flac");
    std::fs::write(&path, b"changed").unwrap();
    let file = ScannedFile::new(path, "01.flac".to_string(), 5, 1);
    let reader = CountingReader::default();

    let error = extract_file_tag_snapshot(&[file], 1, 0, &reader).unwrap_err();

    assert!(matches!(error, ImportError::FileTags { detail } if detail.contains("changed after")));
    assert_eq!(reader.0.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[test]
fn embedded_artwork_becomes_the_file_tags_cover_selection() {
    let snapshot = snapshot(Some(EmbeddedCoverFact {
        source_relative_path: "01.flac".to_string(),
        content_type: ContentType::Jpeg,
        data: vec![1, 2, 3],
    }));

    assert_eq!(
        embedded_cover_selection(&snapshot),
        Some(super::super::CoverSelection::Embedded(
            "01.flac".to_string()
        ))
    );
}

#[test]
fn missing_embedded_artwork_stores_no_file_tags_cover_selection() {
    assert_eq!(embedded_cover_selection(&snapshot(None)), None);
}

#[test]
fn unsupported_snapshot_artwork_is_not_selected_when_applying_file_tags() {
    let snapshot = snapshot(Some(EmbeddedCoverFact {
        source_relative_path: "01.flac".to_string(),
        content_type: ContentType::Bmp,
        data: b"BM".to_vec(),
    }));
    assert_eq!(embedded_cover_selection(&snapshot), None);
    assert!(
        snapshot.embedded_cover.is_some(),
        "the stored observation is retained"
    );
}

const PLACEHOLDER_FLAC: &[u8] = include_bytes!("../../tests/fixtures/flac/01 Test Track 1.flac");
const PLACEHOLDER_M4A: &[u8] =
    include_bytes!("../../test-fixtures/audio-format/placeholder-aac.m4a");

/// A FLAC carrying these Vorbis comments, written to `dir`.
fn flac_with(dir: &Path, comments: &[(&str, &str)]) -> std::path::PathBuf {
    use lofty::config::WriteOptions;
    let path = dir.join("01.flac");
    std::fs::write(&path, PLACEHOLDER_FLAC).unwrap();
    let mut tag = lofty::ogg::tag::VorbisComments::default();
    for (key, value) in comments {
        tag.push((*key).to_string(), (*value).to_string());
    }
    tag.save_to_path(&path, WriteOptions::default()).unwrap();
    path
}

/// An MP4 file carrying these text atoms, written to `dir`.
fn m4a_with(dir: &Path, atoms: &[([u8; 4], &str)]) -> std::path::PathBuf {
    use lofty::config::WriteOptions;
    use lofty::mp4::{Atom, AtomData, AtomIdent, Ilst};
    let path = dir.join("01.m4a");
    std::fs::write(&path, PLACEHOLDER_M4A).unwrap();
    let mut ilst = Ilst::default();
    ilst.insert(Atom::new(
        AtomIdent::Fourcc(*b"\xa9nam"),
        AtomData::UTF8("Track Alpha".to_string()),
    ));
    for (fourcc, value) in atoms {
        ilst.insert(Atom::new(
            AtomIdent::Fourcc(*fourcc),
            AtomData::UTF8((*value).to_string()),
        ));
    }
    ilst.save_to_path(&path, WriteOptions::default()).unwrap();
    path
}

/// A file's label is read off `LABEL`, or `ORGANIZATION` where it has none,
/// and its copyright line as written.
#[test]
fn a_tag_s_label_and_copyright_are_read() {
    let dir = tempfile::TempDir::new().unwrap();
    let read = LoftyFileTagReader
        .read(&flac_with(
            dir.path(),
            &[
                ("LABEL", "Imprint Name"),
                ("COPYRIGHT", "(C) 1999 Imprint Name (P) 1999 Imprint Name"),
            ],
        ))
        .unwrap()
        .tags;
    assert_eq!(read.labels, ["Imprint Name"]);
    assert_eq!(
        read.copyrights,
        ["(C) 1999 Imprint Name (P) 1999 Imprint Name"]
    );
    let read = LoftyFileTagReader
        .read(&flac_with(dir.path(), &[("ORGANIZATION", "Other Imprint")]))
        .unwrap()
        .tags;
    assert_eq!(read.labels, ["Other Imprint"]);
    assert_eq!(read.store, None);
    let read = LoftyFileTagReader
        .read(&flac_with(
            dir.path(),
            &[("LABEL", "Imprint Name"), ("LABEL", "Second Imprint")],
        ))
        .unwrap()
        .tags;
    assert_eq!(read.labels, ["Imprint Name", "Second Imprint"]);
}

/// Bandcamp's own comment marks a Bandcamp download; a comment of any other
/// kind marks nothing.
#[test]
fn bandcamp_s_comment_marks_a_bandcamp_download() {
    let dir = tempfile::TempDir::new().unwrap();
    let read = LoftyFileTagReader
        .read(&flac_with(
            dir.path(),
            &[("COMMENT", "Visit https://artistname.bandcamp.com")],
        ))
        .unwrap();
    assert_eq!(read.tags.store, Some(StoreMarker::Bandcamp));
    let read = LoftyFileTagReader
        .read(&flac_with(dir.path(), &[("COMMENT", "Ripped with care")]))
        .unwrap();
    assert_eq!(read.tags.store, None);
}

/// The atoms iTunes writes only into a purchase mark an iTunes purchase; its
/// catalog ids alone do not.
#[test]
fn an_itunes_purchase_is_marked_by_its_purchase_atoms() {
    let dir = tempfile::TempDir::new().unwrap();
    for atom in [*b"purd", *b"apID", *b"ownr"] {
        let read = LoftyFileTagReader
            .read(&m4a_with(dir.path(), &[(atom, "placeholder")]))
            .unwrap();
        assert_eq!(
            read.tags.store,
            Some(StoreMarker::ITunesPurchase),
            "{atom:?}"
        );
    }
    let read = LoftyFileTagReader
        .read(&m4a_with(dir.path(), &[(*b"cnID", "1")]))
        .unwrap();
    assert_eq!(read.tags.store, None);
}

/// Every value of a multi-valued comment is its own: two `ARTIST` comments
/// are two artists, and a value written twice is stated once.
#[test]
fn every_artist_a_file_names_is_read() {
    let dir = tempfile::TempDir::new().unwrap();
    let read = LoftyFileTagReader
        .read(&flac_with(
            dir.path(),
            &[
                ("ARTIST", "Artist Alpha"),
                ("ARTIST", "Artist Beta"),
                ("ARTIST", "Artist Alpha"),
                ("ALBUMARTIST", "Artist Alpha"),
                ("ALBUMARTIST", " "),
            ],
        ))
        .unwrap()
        .tags;
    assert_eq!(read.track_artists, ["Artist Alpha", "Artist Beta"]);
    assert_eq!(read.album_artists, ["Artist Alpha"]);
}

/// Every date a file's tags state names its year, each once, in the order
/// the keys are read: the recording date, the year, the release date, the
/// original release date.
#[test]
fn every_year_a_file_s_dates_name_is_read() {
    let dir = tempfile::TempDir::new().unwrap();
    let read = LoftyFileTagReader
        .read(&flac_with(
            dir.path(),
            &[
                ("DATE", "2005-03-01"),
                ("ORIGINALDATE", "1971"),
                ("RELEASEDATE", "2005"),
                ("YEAR", "notayear"),
            ],
        ))
        .unwrap()
        .tags;
    assert_eq!(read.years, [2005, 1971]);
    let read = LoftyFileTagReader
        .read(&flac_with(dir.path(), &[("TITLE", "Track Alpha")]))
        .unwrap()
        .tags;
    assert!(read.years.is_empty());
}

/// A date names the first four digits of each run of four or more.
#[test]
fn a_date_names_each_year_it_writes() {
    let years = |date| years_in_date(date).collect::<Vec<_>>();
    assert_eq!(years("1970"), [1970]);
    assert_eq!(years("1970-02-03"), [1970]);
    assert_eq!(years("19700203"), [1970]);
    assert_eq!(years("2000 / 2004"), [2000, 2004]);
    assert!(years("rem 99").is_empty());
    assert!(years("0000").is_empty());
}

/// The compilation flag is read as set only when it says 1.
#[test]
fn the_compilation_flag_is_read() {
    let dir = tempfile::TempDir::new().unwrap();
    let flagged = |value: &str| {
        LoftyFileTagReader
            .read(&flac_with(dir.path(), &[("COMPILATION", value)]))
            .unwrap()
            .tags
            .compilation
    };
    assert!(flagged("1"));
    assert!(!flagged("0"));
}
