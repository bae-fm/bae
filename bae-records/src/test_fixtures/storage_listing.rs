//! Releases and files for the storage listing's tests, and the plan SQLite
//! picks for a read.

use crate::NewReleaseFile;
use bae_store::DbError;

pub const RELEASE_EARLY: &str = "d439555e-9883-4037-a580-48ea33c2c57b";
pub const RELEASE_LATE: &str = "af4421de-5174-45a9-81fc-74110bc5f85d";

/// Two releases on albums titled to sort apart.
pub async fn write_library(records: &crate::Records) {
    super::library::RecordsFixture::new()
        .album(ALBUM_A, "Album Title A", None, &[])
        .album(ALBUM_B, "Album Title B", None, &[])
        .release(ALBUM_A, RELEASE_EARLY)
        .release(ALBUM_B, RELEASE_LATE)
        .write(records)
        .await;
}

/// Seed the release file `id` of `release`, named `name`, staged on this
/// device only from an original of `size` zero bytes written under `dir`.
pub async fn insert_file(
    store: &bae_store::Store,
    dir: &std::path::Path,
    (id, release, name, size): (&str, &str, &str, u64),
) {
    let original = dir.join(id);
    std::fs::File::create_new(&original).unwrap().set_len(size).unwrap();
    super::test_records(store)
        .insert_file_for_test(
            &NewReleaseFile::new(
                release,
                name,
                bae_util::content_type::ContentType::Jpeg,
                id.to_string(),
                chrono::Utc::now(),
            ),
            &original,
        )
        .await
        .unwrap();
}

/// The plan SQLite chooses for `statement`, one detail line per step.
pub async fn plan(db: &bae_store::Store, statement: String) -> Vec<String> {
    db.read(move |sql| {
        sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), [], |row| row.get::<_, String>(3))
            .map_err(DbError::from)
    })
    .await
    .unwrap()
}

pub const ALBUM_A: &str = "4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d";
pub const ALBUM_B: &str = "dab6efe2-9b5c-4034-b95e-73cf49f54b9a";
