//! Audio the tests of this crate and the crates above it import: short FLAC
//! tracks and a 96 kHz one (tests/fixtures/flac), a CUE/APE album beside two
//! FLAC tracks of its own (tests/fixtures/cue_ape), and ALAC and AAC files
//! with a sheet for one of them (test-fixtures/alac); and those files copied
//! with the tags a test wants written on them.

use std::path::PathBuf;

/// The FLAC tracks' folder: `01 Test Track 1.flac` to `03 Test Track 3.flac`
/// and `96khz_test.flac`.
pub fn flac_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/flac"))
}

/// The ALAC and AAC files' folder: `silence-alac.m4a`, `silence-aac.m4a`,
/// and `cue-alac.m4a` with the `cue-alac.cue` that splits it.
pub fn alac_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/test-fixtures/alac"))
}

/// The CUE/APE album's folder: `Test Album.ape`, `Test Album.cue`, and two
/// tracks as FLAC files of their own.
pub fn cue_ape_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cue_ape"))
}

/// A committed test fixture's bytes. Panics naming the file and how to get it
/// back when the checkout lacks it.
pub fn read_fixture(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| {
        panic!(
            "test fixture {} could not be read ({error}); it is committed, so \
             restore it with `git checkout -- {}`",
            path.display(),
            path.display()
        )
    })
}

/// Write one tagged FLAC into `dir` (copied from the test fixture) with the
/// given `title`, so an Unknown-identity import can map it from file metadata.
/// Returns the on-disk bytes after tagging.
pub fn write_tagged_flac(dir: &std::path::Path, filename: &str, title: &str) -> Vec<u8> {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    let fixture = flac_dir().join("01 Test Track 1.flac");
    let flac = read_fixture(&fixture);

    let dest = dir.join(filename);
    std::fs::write(&dest, &flac).unwrap();
    let mut tagged = lofty::read_from_path(&dest).expect("read for tagging");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(title.to_string());
    tag.set_artist("Artist Name".to_string());
    tag.set_album("Album Title".to_string());
    tag.insert_text(ItemKey::AlbumArtist, "Artist Name".to_string());
    tag.set_track(1);
    tagged.insert_tag(tag);
    tagged.save_to_path(&dest, WriteOptions::default()).expect("write tags");

    std::fs::read(&dest).unwrap()
}

/// Copy `source` into `dest_dir/name`, stamp Vorbis-comment tags on it (title,
/// artist, album, album artist, year, track), and return the destination path.
/// For tests that need a tagged audio file derived from a specific fixture.
pub fn copy_and_tag(
    source: &std::path::Path,
    dest_dir: &std::path::Path,
    name: &str,
    title: &str,
    artist: &str,
    album_title: &str,
    album_artist: &str,
    year: u16,
    track: u32,
) -> std::path::PathBuf {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::items::Timestamp;
    use lofty::tag::{Tag, TagType};

    let dest = dest_dir.join(name);
    std::fs::copy(source, &dest).expect("copy fixture");

    let mut tagged = lofty::read_from_path(&dest).expect("read for tagging");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(title.to_string());
    tag.set_artist(artist.to_string());
    tag.set_album(album_title.to_string());
    tag.insert_text(ItemKey::AlbumArtist, album_artist.to_string());
    tag.set_date(Timestamp {
        year,
        month: None,
        day: None,
        hour: None,
        minute: None,
        second: None,
    });
    tag.set_track(track);
    tagged.insert_tag(tag);
    tagged.save_to_path(&dest, WriteOptions::default()).expect("save tags");
    dest
}

/// Write a small solid-color PNG to `path` — a folder cover an import picks up,
/// so a test release actually has art (for exercising cover-embedding paths).
pub fn write_cover_png(path: &std::path::Path) {
    std::fs::write(path, bae_catalogs::test_image_host::cover_png()).expect("write cover png");
}
