//! Test databases on a schema and the rows tests seed them with, for the
//! tests of this crate and every crate that builds on it.

/// The schema this crate's tests open their stores on, and the tests of the
/// crates above it that hold no part of the schema: records' and release
/// storage's parts.
pub const TEST_SCHEMA: bae_store::Schema = bae_store::Schema::new(&[crate::SCHEMA_PARTS]);

/// The records over `store`, with their ids drawn at random.
pub fn test_records(store: &bae_store::Store) -> crate::Records {
    crate::Records::new(store.clone(), std::sync::Arc::new(coven::UuidIds))
}

// Fixture row ids, UUIDs because coven refuses any other synced primary key.
pub const ALBUM_A: &str = "a67c03ad-425f-45e9-8279-0144c852aaa5";
pub const ARTIST_A: &str = "d7d8141f-54ff-467d-8b60-4f34a4d2e528";
pub const ARTIST_ALBUM: &str = "85f70840-aba5-4eb9-8e1a-0d319e53b798";
pub const ARTIST_B: &str = "38fc314c-c130-4120-8ca9-38b870ccef3a";
pub const ARTIST_C: &str = "1b4bafc9-0ece-4538-833e-4ff52feb6ef0";
pub const ARTIST_COMPOSER: &str = "5412b7ad-bdc1-4561-8985-b6d6ef8a2880";
pub const ARTIST_PRIMARY: &str = "7cdf9a34-0746-472b-8c68-0a669c11f2f1";
pub const RELEASE_A: &str = "0252dedb-ee39-4547-8803-438dbeb57a64";
pub const RELEASE_B: &str = "64e79a1f-404a-4c34-809a-a3cb44bf1942";
pub const RELEASE_ROLE_A: &str = "9b72bbbf-621e-41ca-8930-1623b643a20d";
pub const RELEASE_Z: &str = "8aa66d48-65a0-42e4-8c1d-e7481e8c1861";
pub const TRACK_A: &str = "0482872e-d4bf-4080-8426-441a0a3e71fc";
pub const TRACK_B: &str = "04676261-1659-47b1-879c-2947c52f4a8d";
pub const TRACK_ROLE_A: &str = "fa0c8483-f09a-4b69-8903-b1ebcdc31322";
pub const TRACK_WORK_A: &str = "d410a973-6a19-4ad3-87d8-b0c8c13d6015";
pub const WORK_A: &str = "432c8996-8af0-43dc-868a-822a256f65c4";

/// Store a real cover holding `content` as a coven file its row references,
/// and return the version reads report for it.
pub async fn write_cover(records: &crate::Records, release: &str, content: &str) -> String {
    let bytes = content.as_bytes();
    let image = crate::DbLibraryImage::cover(release);
    records.write_library_image(&image, bytes).await.unwrap();
    stored_image_version(records, crate::synced::COVERS_NAMESPACE, release).await
}

/// The version reads should report for the image on `table`'s row `id`: the
/// coven file its row references, then `@` and the device that has it while
/// it is on one device only, read from coven's own record of the file rather
/// than from the version query.
pub async fn stored_image_version(records: &crate::Records, table: &str, id: &str) -> String {
    let image_type = match table {
        crate::synced::COVERS_NAMESPACE => crate::LibraryImageType::Cover,
        crate::synced::ARTIST_IMAGES_NAMESPACE => crate::LibraryImageType::Artist,
        other => panic!("{other} holds no library images"),
    };
    let file = records.library_image_file(id, &image_type).await.unwrap().expect("the image row");
    match file.place() {
        coven::FilePlace::Cloud { .. } => file.id().0.to_string(),
        coven::FilePlace::DeviceOnly { device } => {
            format!("{}@{}", file.id().0, hex::encode(device.0.to_be_bytes()))
        }
    }
}

/// A breaking change to the records' schema, as an update of the app brings
/// one: an index on a synced table.
pub const WORKS_BY_TITLE: &str = "CREATE INDEX works_by_title ON works(title)";

/// The album `album_id`, titled `title`, holding `releases` in that order,
/// none with a track.
pub async fn album_with_releases(
    records: &crate::Records,
    album_id: &str,
    title: &str,
    releases: &[&str],
) {
    let album = crate::DbAlbum {
        id: album_id.to_string(),
        ..crate::DbAlbum::new_test(title, &coven::UuidIds)
    };
    records.insert_album(&album).await.unwrap();
    for release in releases {
        records.insert_release(&crate::DbRelease::new_test(album_id, release)).await.unwrap();
    }
}

/// The album `album_id`, titled `title`, holding the one release
/// `release_id`, whose one CD holds `tracks` (each an id and a title) in
/// order.
pub async fn album_with_tracks(
    records: &crate::Records,
    (album_id, title): (&str, &str),
    release_id: &str,
    tracks: &[(&str, &str)],
) {
    album_with_releases(records, album_id, title, &[release_id]).await;
    records.insert_release_medium(&crate::DbReleaseMedium::new_test(release_id)).await.unwrap();
    for ((track_id, title), number) in tracks.iter().zip(1..) {
        records
            .insert_track(&crate::DbTrack::new_test(release_id, track_id, title, number))
            .await
            .unwrap();
    }
}

/// A credit as a catalog or a file names an artist: `name`, with no sort
/// name or catalog id, for [`crate::Records::find_or_create_artists`] to
/// resolve.
pub fn credit(name: &str) -> crate::DbArtist {
    crate::DbArtist {
        id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    }
}

/// Delete `release_id` as the library deletes a release: its tracks, files
/// and credits go with it in one write.
pub async fn delete_release(records: &crate::Records, release_id: &str) {
    let plan = records.plan_release_deletion(release_id).await.unwrap().unwrap();
    records.delete_release_with_cleanup(plan).await.unwrap();
}

/// `ARTIST_PRIMARY` owning `ALBUM_A`, whose primary release is `RELEASE_A`.
pub fn artist_album_release() -> library::RecordsFixture {
    library::RecordsFixture::new()
        .artist(ARTIST_PRIMARY, "Artist Name Primary")
        .album(ALBUM_A, "Album Title A", Some(2026), &[ARTIST_PRIMARY])
        .release(ALBUM_A, RELEASE_A)
}

/// Write [`artist_album_release`] to `store`.
pub async fn seed_artist_album_release(store: &bae_store::Store) {
    artist_album_release().write(&test_records(store)).await;
}

pub mod aggregate_ordering;
pub mod album_listing;
pub mod artists_apart;
pub mod bulk_library;
pub mod bulk_tracks;
pub mod chunked_tracks;
pub mod composed_work;
pub mod labels;
pub mod library;
pub mod live_query;
pub mod shaped_library;
pub mod storage_listing;

/// Drop the triggers keeping `release_storage` from a closed library file, to
/// time a write without them.
pub fn drop_release_storage_triggers(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::drop_triggers_of(
        directory,
        "release_storage",
        include_str!("release_storage.sql"),
    );
}
