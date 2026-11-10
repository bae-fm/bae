//! Libraries seeded for the reads that aggregate and order a release's
//! credits, tracks and files.

use super::library::RecordsFixture;
use super::{
    test_records, ALBUM_A, ARTIST_PRIMARY, RELEASE_A, RELEASE_B, RELEASE_Z, TRACK_A, TRACK_B,
    WORK_A,
};
use crate::NewReleaseFile;

const ARTIST_FIRST: &str = "7e7d8df5-8292-4287-80be-7abd24f5a992";
const ARTIST_SECOND: &str = "863911c6-a6b6-40a7-8096-b85eb877f7c7";
const COMPOSER_FIRST: &str = "c0770501-2551-4e87-8801-93f780248cf3";
const COMPOSER_SECOND: &str = "0d0e8916-becd-4d2c-89e0-7cc5c7005f83";
const TRACK_ARTIST_FIRST: &str = "5b5f8c38-5237-4187-895c-28b1b2a43672";
const TRACK_ARTIST_SECOND: &str = "8ccac2a7-7e60-4f52-881e-0b349ff78cc5";

/// The library the reads that aggregate and order a release's credits and
/// tracks are tested on, on `schema`: an album credited to three artists,
/// with three releases, two of them at one place in its order, and a work
/// credited to two composers. Each list's rows are written in an order other
/// than their own, so a read that orders them shows it does.
pub async fn aggregate_db(schema: bae_store::Schema) -> (bae_store::Store, tempfile::TempDir) {
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    RecordsFixture::new()
        .artist(ARTIST_PRIMARY, "Artist Name Primary")
        .artist(ARTIST_FIRST, "Artist Name First")
        .artist(ARTIST_SECOND, "Artist Name Second")
        .artist(COMPOSER_FIRST, "Composer Name First")
        .artist(COMPOSER_SECOND, "Composer Name Second")
        .album(ALBUM_A, "Album Title A", Some(2026), &[ARTIST_PRIMARY, ARTIST_FIRST, ARTIST_SECOND])
        .release(ALBUM_A, RELEASE_Z)
        .at("2026-01-01T00:00:01Z")
        .release(ALBUM_A, RELEASE_B)
        .release(ALBUM_A, RELEASE_A)
        .work(WORK_A, "Work Title A")
        .composer(WORK_A, COMPOSER_FIRST)
        .composer(WORK_A, COMPOSER_SECOND)
        .edit(|rows| {
            rows.album_artists.reverse();
            rows.work_artists.reverse();
        })
        .write(&test_records(&db))
        .await;
    plant_release_at_a_shared_place(tmp.path());
    (db, tmp)
}

/// Plant `RELEASE_A` at `RELEASE_B`'s place in their album's order, in the
/// library stored under `directory`: the tie two devices leave when each
/// adds a release to one album while apart. No write on one device leaves
/// it, as each puts its release after the album's others. `RELEASE_B` was
/// written first, so only a read that breaks the tie by id lists
/// `RELEASE_A` before it.
fn plant_release_at_a_shared_place(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "releases",
        &format!(
            "UPDATE releases SET album_order = \
             (SELECT album_order FROM releases WHERE id = '{RELEASE_B}') \
             WHERE id = '{RELEASE_A}';"
        ),
    );
}

/// `RELEASE_A` of the shared artist's album, holding `tracks` (each an id
/// and a title) in order, the first credited to two artists of its own,
/// written in an order other than theirs.
fn credited_tracks(tracks: &[(&str, &str)]) -> RecordsFixture {
    super::artist_album_release()
        .artist(TRACK_ARTIST_FIRST, "Track Artist Name First")
        .artist(TRACK_ARTIST_SECOND, "Track Artist Name Second")
        .tracks(RELEASE_A, tracks)
        .track_credits(TRACK_A, &[TRACK_ARTIST_FIRST, TRACK_ARTIST_SECOND])
        .edit(|rows| rows.track_artists.reverse())
}

/// The library a queue item's track artists are read from, on `schema`:
/// [`credited_tracks`] with one track.
pub async fn queue_db(schema: bae_store::Schema) -> (bae_store::Store, tempfile::TempDir) {
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    credited_tracks(&[(TRACK_A, "Track Title A")]).write(&test_records(&db)).await;
    (db, tmp)
}

/// The library a release's detail is read from, on `schema`:
/// [`credited_tracks`] with a second track, credited to none of its own.
pub async fn release_detail_db(schema: bae_store::Schema) -> (bae_store::Store, tempfile::TempDir) {
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    credited_tracks(&[(TRACK_A, "Track Title A"), (TRACK_B, "Track Title B")])
        .write(&test_records(&db))
        .await;
    (db, tmp)
}

/// Seed a release file, staged on this device only from an original of
/// `size` zero bytes written under `dir`.
pub async fn insert_file(
    db: &bae_store::Store,
    dir: &std::path::Path,
    file: NewReleaseFile,
    size: u64,
) {
    let original = dir.join(&file.id);
    std::fs::File::create_new(&original).unwrap().set_len(size).unwrap();
    super::test_records(db).insert_file_for_test(&file, &original).await.unwrap();
}

/// The names `insert_files_out_of_natural_order` seeds, in natural order,
/// case aside.
pub const FILES_IN_NATURAL_ORDER: [&str; 5] =
    ["album.cue", "Back.jpg", "cover.jpg", "Track 2.flac", "Track 10.flac"];

/// Seed `RELEASE_A` with the files of `FILES_IN_NATURAL_ORDER`, inserted out
/// of order, mixed case, with a two-digit number.
pub async fn insert_files_out_of_natural_order(db: &bae_store::Store, dir: &std::path::Path) {
    for (i, name) in
        ["Track 10.flac", "cover.jpg", "Track 2.flac", "Back.jpg", "album.cue"].iter().enumerate()
    {
        let mime = if name.ends_with(".flac") {
            "audio/flac"
        } else if name.ends_with(".cue") {
            "application/x-cue"
        } else {
            "image/jpeg"
        };
        insert_file(
            db,
            dir,
            NewReleaseFile {
                id: bae_util::test_ids::test_uuid(&format!("ordering-file-{i}")),
                release_id: RELEASE_A.to_string(),
                original_filename: name.to_string(),
                content_type: bae_util::content_type::ContentType::from_mime(mime),
                role: crate::ReleaseFileRole::Other,
                source_audio: None,
                created_at: chrono::Utc::now(),
            },
            1,
        )
        .await;
    }
}
