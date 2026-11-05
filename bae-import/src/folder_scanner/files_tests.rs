//! A release's files: the facts each format gives its source audio, and the
//! content hash its files are known by.

use super::*;
use crate::folder_scanner::tests::*;

fn mp3_with_id3_padding(padding: usize) -> Vec<u8> {
    let mut bytes = fake_mp3();
    assert_eq!(&bytes[..3], b"ID3", "MP3 fixture must begin with ID3v2");

    let tag_size = bytes[6..10].iter().fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
    let padded_size = tag_size + padding;
    assert!(padded_size < 1 << 28, "ID3v2 size is a synchsafe integer");

    let audio_offset = 10 + tag_size;
    bytes.splice(audio_offset..audio_offset, std::iter::repeat_n(0, padding));
    for (index, shift) in [21, 14, 7, 0].into_iter().enumerate() {
        bytes[6 + index] = ((padded_size >> shift) & 0x7f) as u8;
    }
    bytes
}

#[test]
fn mp3_tag_sizes_do_not_make_uniform_audio_mixed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let album = temp_dir.path().join("Album");
    std::fs::create_dir(&album).unwrap();
    std::fs::write(album.join("01.mp3"), mp3_with_id3_padding(16 * 1024)).unwrap();
    std::fs::write(album.join("02.mp3"), mp3_with_id3_padding(64 * 1024)).unwrap();

    let candidates = scan_valid(temp_dir.path().to_path_buf());
    let candidate = candidates.first().expect("scanner returns the album");
    let summary =
        candidate.files.source_audio_summary().expect("MP3 files have source-audio facts");

    let bae_records::source_audio::SourceAudioSummary::Uniform { descriptor } = summary else {
        panic!("equal MP3 streams with different tag sizes must be uniform");
    };
    assert_eq!(descriptor.format.bitrate_kbps, Some(320));
}

#[test]
fn opus_bitrate_comes_from_audio_packets_when_the_stream_has_no_declared_rate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let album = temp_dir.path().join("Album");
    std::fs::create_dir(&album).unwrap();
    std::fs::copy(
        bae_audio::test_fixtures::audio_format_dir().join("placeholder-opus.opus"),
        album.join("01.opus"),
    )
    .unwrap();

    let candidates = scan_valid(temp_dir.path().to_path_buf());
    let candidate = candidates.first().expect("scanner returns the album");
    let summary = candidate.files.source_audio_summary().expect("Opus file has source-audio facts");

    let bae_records::source_audio::SourceAudioSummary::Uniform { descriptor } = summary else {
        panic!("one Opus stream must be uniform");
    };
    assert_eq!(descriptor.format.bitrate_kbps, Some(1));
}

#[test]
fn content_hash_is_location_independent_and_size_sensitive() {
    let make = |root: &str, second_size: u64| CategorizedFiles {
        files: vec![
            audio_entry(&format!("{root}/01.flac"), "01.flac", 1000),
            audio_entry(&format!("{root}/02.flac"), "02.flac", second_size),
        ],
        parts: Vec::new(),
    };

    // The same relative structure under two different parent folders hashes
    // identically — the fingerprint follows the rip, not where it sits.
    let a = make("/Volumes/Music/Release", 2000);
    let b = make("/tmp/import_source/Release", 2000);
    assert_eq!(a.content_hash(), b.content_hash());

    // A single differing file size flips the hash.
    let c = make("/Volumes/Music/Release", 2001);
    assert_ne!(a.content_hash(), c.content_hash());
}

#[test]
fn content_hash_changes_with_file_modification_time() {
    let entry = |modified_at_ns| CandidateFile {
        file: ScannedFile::new(
            PathBuf::from("/music/01.flac"),
            "01.flac".to_string(),
            1000,
            modified_at_ns,
        )
        .with_test_flac_audio(),
        role: FileRole::Audio,
    };
    let before = CategorizedFiles { files: vec![entry(1)], parts: Vec::new() };
    let after = CategorizedFiles { files: vec![entry(2)], parts: Vec::new() };

    assert_ne!(before.content_hash(), after.content_hash());
}

#[test]
fn content_hash_is_independent_of_discovery_order() {
    let entry = |name: &str, size: u64, role: FileRole| CandidateFile {
        file: ScannedFile::new(PathBuf::from(name), name.to_string(), size, 1),
        role,
    };
    let forward = CategorizedFiles {
        files: vec![
            entry("01.flac", 1, FileRole::Audio),
            entry("02.flac", 2, FileRole::Audio),
            entry("cover.jpg", 3, FileRole::Artwork),
            entry("notes.txt", 4, FileRole::Document),
        ],
        parts: Vec::new(),
    };
    let shuffled = CategorizedFiles {
        files: vec![
            entry("notes.txt", 4, FileRole::Document),
            entry("02.flac", 2, FileRole::Audio),
            entry("cover.jpg", 3, FileRole::Artwork),
            entry("01.flac", 1, FileRole::Audio),
        ],
        parts: Vec::new(),
    };
    assert_eq!(forward.content_hash(), shuffled.content_hash());
}

/// The hash covers every file the release uploads, including audio no sheet
/// references — which the either/or silently dropped from both. This is the
/// test that fails if that omission is ever reintroduced.
#[test]
fn content_hash_covers_audio_no_sheet_references() {
    let build = |dir: &Path, with_bonus: bool| {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("Album.flac"), fake_flac()).unwrap();
        std::fs::write(dir.join("Album.cue"), make_cue_content("Album.flac", "Album Title"))
            .unwrap();
        if with_bonus {
            std::fs::write(dir.join("bonus.flac"), fake_flac()).unwrap();
        }
        scan_files(dir).content_hash()
    };
    let tmp = tempfile::tempdir().unwrap();
    let plain = build(&tmp.path().join("Plain"), false);
    let with_bonus = build(&tmp.path().join("Bonus"), true);
    assert_ne!(plain, with_bonus, "audio no sheet references must count toward the hash",);
}
