//! A library of two artists and three albums, for the album and artist
//! listings' tests.

use super::library::RecordsFixture;

pub const BRAVO: &str = "f392a38b-f264-41a0-a225-c6935b8a8262";
pub const ALPHA: &str = "4309b580-a6bf-455e-92ea-ec58ee7129e7";
pub const ALBUM_ONE: &str = "07d22f7d-3980-4c81-9d27-8fcd50ee3464";
pub const ALBUM_TWO: &str = "7cc40503-11b4-4ba3-8cec-45ba0498232b";
pub const ALBUM_THREE: &str = "c217fb46-e6a3-4eaf-bc0a-ac9036440dc7";
pub const RELEASE_ONE: &str = "5a905d5d-d400-42f3-a9a8-b858f18dcbc3";
pub const RELEASE_TWO: &str = "3a629e97-79c5-42da-99ac-09cfb3733dd7";
pub const RELEASE_THREE: &str = "7cc7aeee-e43c-469d-aa7a-990ea258bd80";

/// Two artists and three albums: the first by Bravo (2001), the second by
/// Alpha (year unknown), the third by Bravo and Alpha in that order (1999),
/// each on one release, each album added a second after the one before and
/// every release at the first moment.
pub async fn write_listed(records: &crate::Records) {
    RecordsFixture::new()
        .artist(BRAVO, "Artist Name Bravo")
        .artist(ALPHA, "Artist Name Alpha")
        .release(ALBUM_ONE, RELEASE_ONE)
        .release(ALBUM_TWO, RELEASE_TWO)
        .release(ALBUM_THREE, RELEASE_THREE)
        .at("2026-01-01T00:00:01Z")
        .album(ALBUM_ONE, "Album Title One", Some(2001), &[BRAVO])
        .at("2026-01-01T00:00:02Z")
        .album(ALBUM_TWO, "Album Title Two", None, &[ALPHA])
        .at("2026-01-01T00:00:03Z")
        .album(ALBUM_THREE, "Album Title Three", Some(1999), &[BRAVO, ALPHA])
        .write(records)
        .await;
}

/// [`write_listed`]'s library on `schema`.
pub async fn listed_db(schema: bae_store::Schema) -> (bae_store::Store, tempfile::TempDir) {
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    write_listed(&super::test_records(&db)).await;
    (db, tmp)
}
