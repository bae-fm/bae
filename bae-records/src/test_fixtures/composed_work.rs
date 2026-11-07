//! A library holding one composed work: an album artist's album on one
//! release with one track, which performs a work part credited to its
//! composer, and the work it is part of.

use super::library::RecordsFixture;
use super::{
    test_records, ALBUM_A, ARTIST_ALBUM, ARTIST_COMPOSER, RELEASE_A, TRACK_A, TRACK_WORK_A,
};
use bae_types::pressing::{MediaCount, Medium};

pub const WORK_CHILD_A: &str = "f63d8e66-6a81-4a67-8005-1fbe870f27eb";
pub const WORK_PARENT_A: &str = "6b05af7a-ee0c-4f12-8938-1d5536697271";

/// An album artist's album on one release with one track, which performs a
/// work part whose composer is credited on it, and the work it is part of, on
/// `schema`.
pub async fn composed_work_db(schema: bae_store::Schema) -> (bae_store::Store, tempfile::TempDir) {
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    RecordsFixture::new()
        .artist(ARTIST_ALBUM, "Album Artist A")
        .sorted_artist(ARTIST_COMPOSER, "Displayed Composer A", "Hidden Composer Sort A")
        .catalog_ids(ARTIST_COMPOSER, None, Some("mb-artist-composer-a"))
        .album(ALBUM_A, "Album Title A", Some(2026), &[ARTIST_ALBUM])
        .release(ALBUM_A, RELEASE_A)
        .tracks(RELEASE_A, &[(TRACK_A, "Track Title A")])
        .work(WORK_PARENT_A, "Parent Work A")
        .work_of_type(WORK_CHILD_A, "Displayed Work A", "part")
        .composer(WORK_CHILD_A, ARTIST_COMPOSER)
        .part(WORK_PARENT_A, WORK_CHILD_A)
        .performs(TRACK_A, WORK_CHILD_A)
        .edit(|rows| {
            let pressing = &mut rows.releases[0].pressing;
            pressing.year = Some(2026);
            pressing.facts.media = vec![MediaCount { medium: Medium::Cd, count: 1 }];
            rows.track_works[0].id = TRACK_WORK_A.to_string();
        })
        .write(&test_records(&db))
        .await;
    (db, tmp)
}
