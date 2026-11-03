//! Audio files and images on disk.

/// The FLAC fixture tree lives in bae-core, so it is reached relative to the
/// workspace root — `CARGO_MANIFEST_DIR` here is this crate, not bae-core.
fn bae_core_fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("bae-test-support sits one level under the workspace root")
        .join("bae-core/tests/fixtures")
}

/// A committed test fixture's bytes. Panics naming the file and how to get it
/// back when the checkout lacks it.
pub fn read_fixture(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| {
        panic!(
            "test fixture {} could not be read ({error}); it is committed under \
             bae-core/tests/fixtures, so restore it with \
             `git checkout -- bae-core/tests/fixtures`",
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

    let fixture = bae_core_fixtures().join("flac/01 Test Track 1.flac");
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

pub async fn read_cover_image_bytes(
    mgr: &bae_core::library::LibraryManager,
    release_id: &str,
) -> Option<Vec<u8>> {
    mgr.read_cover_image_bytes(release_id).await.unwrap()
}
