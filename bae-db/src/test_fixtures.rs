//! Test databases with bae's schema and the rows tests seed them with, for
//! the tests of this crate and every crate that builds on it.

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

/// A database in a fresh temp directory, under the real clock. The `TempDir`
/// owns the file, so it must outlive the handle.
pub async fn temp_db() -> (crate::Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = crate::Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    (db, tmp)
}

/// The instant the import fixtures pin their clock to.
pub fn fixed_now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

/// `temp_db` under that pinned clock instead of the system one.
pub async fn empty_db() -> (crate::Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = crate::Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::FixedClock::new((fixed_now()).into())),
    )
    .await
    .unwrap();
    (db, tmp)
}

/// Run one statement against the test database, panicking if it fails.
pub async fn exec(db: &crate::Database, sql: &str, args: &[&str]) {
    let sql = sql.to_string();
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    db.call(move |conn| {
        conn.execute(&sql, rusqlite::params_from_iter(args))
            .map(|_| ())
            .map_err(crate::DbError::from)
    })
    .await
    .unwrap();
}

/// How many rows `table` holds.
pub async fn row_count(db: &crate::Database, table: &str) -> usize {
    let query = format!("SELECT COUNT(*) FROM \"{table}\"");
    let count: i64 = db
        .read(move |sql| sql.query_row(&query, [], |row| row.get(0)).map_err(crate::DbError::from))
        .await
        .unwrap();
    usize::try_from(count).expect("a row count is never negative")
}

/// `exec` for a multi-statement script, which takes no bound parameters.
pub async fn exec_batch(db: &crate::Database, sql: &str) {
    let sql = sql.to_string();
    db.call(move |conn| conn.execute_batch(&sql).map_err(crate::DbError::from)).await.unwrap();
}

/// Store a real cover holding `content` as a coven file its row references,
/// and return the version reads report for it.
pub async fn write_cover(db: &crate::Database, release: &str, content: &str) -> String {
    let bytes = content.as_bytes();
    let image = crate::DbLibraryImage::cover(release);
    db.write_library_image(&image, bytes).await.unwrap();
    stored_image_version(db, crate::synced::COVERS_NAMESPACE, release).await
}

/// The version reads should report for the image on `table`'s row `id`: the
/// coven file its row references, then `@` and the device that has it while
/// it is on one device only, read from coven's own record of the file rather
/// than from the version query.
pub async fn stored_image_version(db: &crate::Database, table: &str, id: &str) -> String {
    let image_type = match table {
        crate::synced::COVERS_NAMESPACE => crate::LibraryImageType::Cover,
        crate::synced::ARTIST_IMAGES_NAMESPACE => crate::LibraryImageType::Artist,
        other => panic!("{other} holds no library images"),
    };
    let file = db.library_image_file(id, &image_type).await.unwrap().expect("the image row");
    match file.place() {
        coven::FilePlace::Cloud { .. } => file.id().0.to_string(),
        coven::FilePlace::DeviceOnly { device } => {
            format!("{}@{}", file.id().0, hex::encode(device.0.to_be_bytes()))
        }
    }
}

/// `ARTIST_PRIMARY` owning `ALBUM_A`, whose primary release is `RELEASE_A`.
pub fn seed_artist_album_release(conn: &coven::CovenWrite<'_>) -> Result<(), crate::DbError> {
    conn.execute_batch(
        "
        INSERT INTO artists (id, name, name_key, created_at) VALUES ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'artist name primary', '2026-01-01T00:00:00Z');

        INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', 2026, 0, '2026-01-01T00:00:00Z');
            INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('340983b5-a21a-4ee4-ae87-8433261ec057', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 0, '2026-01-01T00:00:00Z');

        INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '2026-01-01T00:00:00Z', 0);
        ",
    )?;
    Ok(())
}

pub mod aggregate_ordering;
pub mod album_listing;
pub mod chunked_tracks;
pub mod composed_work;
pub mod labels;
pub mod live_query;
pub mod storage_listing;
pub mod window_cost;
