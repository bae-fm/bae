//! Audio the tests of this crate and the crates above it import: short FLAC
//! tracks and a 96 kHz one (tests/fixtures/flac), a CUE/APE album beside two
//! FLAC tracks of its own (tests/fixtures/cue_ape), and ALAC and AAC files
//! with a sheet for one of them (test-fixtures/alac); and those files copied
//! with the tags a test wants written on them.

use std::path::{Path, PathBuf};

use bae_util::test_files::{read_fixture, TestFolder};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFile};
use lofty::tag::TagExt;

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

/// The text of `path`, a file committed to the repo.
pub fn read_fixture_text(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    String::from_utf8(read_fixture(path))
        .unwrap_or_else(|error| panic!("test fixture {} is not UTF-8: {error}", path.display()))
}

/// Copy the files of bae-audio's CUE/FLAC album, a FLAC image of a disc with
/// its sheet and two tracks as FLAC files of their own, to `folder`.
#[cfg(test)]
pub(crate) fn copy_cue_flac_album(folder: &TestFolder) {
    for name in [
        "02 Test Artist - Track Two (White Noise).flac",
        "03 Test Artist - Track Three (Brown Noise).flac",
        "Test Album.cue",
        "Test Album.flac",
    ] {
        folder.copy_fixture(bae_audio::test_fixtures::cue_flac_dir().join(name), name);
    }
}

/// The tags of the audio file at `path` in `folder`.
pub fn read_tagged(folder: &TestFolder, path: impl AsRef<Path>) -> TaggedFile {
    lofty::read_from(&mut folder.open(path)).expect("read for tagging")
}

/// Write `tagged`'s tags to the audio file at `path` in `folder`.
pub fn save_tagged(folder: &TestFolder, path: impl AsRef<Path>, tagged: &TaggedFile) {
    let mut file = folder.open_with(path, std::fs::OpenOptions::new().read(true).write(true));
    tagged.save_to(&mut file, WriteOptions::default()).expect("write tags");
}

/// Write one tag of a kind of its own, such as `VorbisComments` or `Ilst`, to
/// the audio file at `path` in `folder`.
pub fn save_tag(folder: &TestFolder, path: impl AsRef<Path>, tag: &impl TagExt) {
    let mut file = folder.open_with(path, std::fs::OpenOptions::new().read(true).write(true));
    tag.save_to(&mut file, WriteOptions::default()).expect("write tag");
}

/// The bytes of a FLAC (a copy of the test fixture) carrying `title` as its
/// title, so an Unknown-identity import can map it from file metadata.
pub fn tagged_flac_bytes(title: &str) -> Vec<u8> {
    use lofty::prelude::*;
    use lofty::probe::Probe;
    use lofty::tag::{Tag, TagType};

    let mut flac = std::io::Cursor::new(read_fixture(flac_dir().join("01 Test Track 1.flac")));
    let mut tagged = Probe::new(&mut flac)
        .guess_file_type()
        .expect("guess the type")
        .read()
        .expect("read for tagging");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(title.to_string());
    tag.set_artist("Artist Name".to_string());
    tag.set_album("Album Title".to_string());
    tag.insert_text(ItemKey::AlbumArtist, "Artist Name".to_string());
    tag.set_track(1);
    tagged.insert_tag(tag);
    tagged.save_to(&mut flac, WriteOptions::default()).expect("write tags");
    flac.into_inner()
}

/// Copy the fixture `source` to `dest` in `folder`, stamp Vorbis-comment tags
/// on it (title, artist, album, album artist, year, track), and return the
/// destination's full path. For tests that need a tagged audio file derived
/// from a specific fixture.
pub fn copy_and_tag(
    source: &Path,
    folder: &TestFolder,
    dest: impl AsRef<Path>,
    title: &str,
    artist: &str,
    album_title: &str,
    album_artist: &str,
    year: u16,
    track: u32,
) -> PathBuf {
    use lofty::prelude::*;
    use lofty::tag::items::Timestamp;
    use lofty::tag::{Tag, TagType};

    let dest = folder.copy_fixture(source, dest);
    let mut tagged = read_tagged(folder, &dest);
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
    save_tagged(folder, &dest, &tagged);
    dest
}

/// A small solid-color PNG — a folder cover an import picks up, so a test
/// release actually has art (for exercising cover-embedding paths).
pub fn cover_png_bytes() -> Vec<u8> {
    bae_catalogs::test_image_host::cover_png()
}

/// Write the cover PNG to `path` in `folder`.
pub fn write_cover_png(folder: &TestFolder, path: impl AsRef<Path>) {
    folder.write(path, cover_png_bytes());
}
