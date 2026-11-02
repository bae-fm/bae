// Fixture row ids, UUIDs because coven refuses any other synced primary key.
const ALBUM_1: &str = "9644b84d-94b2-4b3b-863a-d6583931920c";
pub(crate) const ALBUM_A: &str = "a67c03ad-425f-45e9-8279-0144c852aaa5";
const ARTIST_1: &str = "6c441836-aef7-4239-8a84-5336c4cce52c";
pub(crate) const ARTIST_A: &str = "d7d8141f-54ff-467d-8b60-4f34a4d2e528";
pub(crate) const ARTIST_ALBUM: &str = "85f70840-aba5-4eb9-8e1a-0d319e53b798";
pub(crate) const ARTIST_B: &str = "38fc314c-c130-4120-8ca9-38b870ccef3a";
pub(crate) const ARTIST_C: &str = "1b4bafc9-0ece-4538-833e-4ff52feb6ef0";
pub(crate) const ARTIST_COMPOSER: &str = "5412b7ad-bdc1-4561-8985-b6d6ef8a2880";
pub(crate) const ARTIST_PRIMARY: &str = "7cdf9a34-0746-472b-8c68-0a669c11f2f1";
const RELEASE_1: &str = "c0218676-4c47-4eb7-8d65-57a8d328c3d1";
pub(crate) const RELEASE_A: &str = "0252dedb-ee39-4547-8803-438dbeb57a64";
pub(crate) const RELEASE_B: &str = "64e79a1f-404a-4c34-809a-a3cb44bf1942";
const RELEASE_LONELY: &str = "fcf4be32-159f-4790-87a1-697700a74462";
const RELEASE_OTHER: &str = "ce596bd7-be97-4416-8b6d-47f315bae466";
pub(crate) const RELEASE_ROLE_A: &str = "9b72bbbf-621e-41ca-8930-1623b643a20d";
pub(crate) const RELEASE_Z: &str = "8aa66d48-65a0-42e4-8c1d-e7481e8c1861";
pub(crate) const TRACK_A: &str = "0482872e-d4bf-4080-8426-441a0a3e71fc";
pub(crate) const TRACK_B: &str = "04676261-1659-47b1-879c-2947c52f4a8d";
const TRACK_LONELY: &str = "03c41035-ce18-4fa0-8e83-c446df26a551";
const TRACK_OTHER: &str = "69e67928-545a-4dcf-8ae7-ef7778331231";
pub(crate) const TRACK_ROLE_A: &str = "fa0c8483-f09a-4b69-8903-b1ebcdc31322";
pub(crate) const TRACK_WORK_A: &str = "d410a973-6a19-4ad3-87d8-b0c8c13d6015";
pub(crate) const WORK_A: &str = "432c8996-8af0-43dc-868a-822a256f65c4";

/// A database in a fresh temp directory, under the real clock. The `TempDir`
/// owns the file, so it must outlive the handle.
pub(crate) async fn temp_db() -> (super::Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = super::Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    (db, tmp)
}

/// The instant the import fixtures pin their clock to.
pub(crate) fn fixed_now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

/// `temp_db` under that pinned clock instead of the system one.
pub(crate) async fn empty_db() -> (super::Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = super::Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::FixedClock::new((fixed_now()).into())),
    )
    .await
    .unwrap();
    (db, tmp)
}

/// The queue rows of `track_ids` queued in Up Next, as the queue value shows
/// them.
async fn queue_items(db: &super::Database, track_ids: &[&str]) -> Vec<crate::queue::QueueItem> {
    crate::db::PlaybackQueueTables::new(db.clone())
        .add_manual(track_ids.iter().map(|id| id.to_string()).collect(), super::ManualPlace::End)
        .await
        .unwrap();
    db.queue_value().await.unwrap().manual
}

/// Run one statement against the test database, panicking if it fails.
pub(crate) async fn exec(db: &super::Database, sql: &str, args: &[&str]) {
    let sql = sql.to_string();
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    db.call(move |conn| {
        conn.execute(&sql, rusqlite::params_from_iter(args))
            .map(|_| ())
            .map_err(crate::db::DbError::from)
    })
    .await
    .unwrap();
}

/// `exec` for a multi-statement script, which takes no bound parameters.
pub(crate) async fn exec_batch(db: &super::Database, sql: &str) {
    let sql = sql.to_string();
    db.call(move |conn| conn.execute_batch(&sql).map_err(crate::db::DbError::from)).await.unwrap();
}

/// Store a real cover holding `content` as a coven file its row references,
/// and return the version reads report for it.
pub(crate) async fn write_cover(db: &super::Database, release: &str, content: &str) -> String {
    let bytes = content.as_bytes();
    let image = crate::db::DbLibraryImage::cover(release, fixed_now());
    db.write_library_image(&image, bytes).await.unwrap();
    stored_image_version(db, crate::db::synced::COVERS_NAMESPACE, release).await
}

/// The version reads should report for the image on `table`'s row `id`: the
/// coven file its row references, then `@` and the device that has it while
/// it is on one device only, read from coven's own record of the file rather
/// than from the version query.
async fn stored_image_version(db: &super::Database, table: &str, id: &str) -> String {
    let image_type = match table {
        crate::db::synced::COVERS_NAMESPACE => crate::db::LibraryImageType::Cover,
        crate::db::synced::ARTIST_IMAGES_NAMESPACE => crate::db::LibraryImageType::Artist,
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
pub(crate) fn seed_artist_album_release(
    conn: &coven::CovenWrite<'_>,
) -> Result<(), crate::db::DbError> {
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

#[cfg(test)]
mod playback_queue_tests;

#[cfg(test)]
mod playback_now_tests;

#[cfg(test)]
mod playback_queue_additions_tests;

#[cfg(test)]
mod playback_timeline_tests;

#[cfg(test)]
mod playback_queue_bench;

#[cfg(test)]
mod release_storage_action_tests;

#[cfg(test)]
mod cast_devices_tests;

#[cfg(test)]
mod sync_start_failure_tests;

#[cfg(test)]
mod personal_order_tests;

#[cfg(test)]
mod in_clause_chunking_tests;

#[cfg(test)]
pub(crate) mod aggregate_ordering_tests;

#[cfg(test)]
mod connection_boundary_tests;

#[cfg(test)]
mod row_mapper_error_tests;

#[cfg(test)]
pub(crate) mod album_listing_tests;

#[cfg(test)]
mod file_row_tests;
mod queued_upload_reads_tests;
pub(crate) mod storage_listing_tests;

#[cfg(test)]
mod fact_ids_tests;

#[cfg(test)]
mod queue_cover_tests;

#[cfg(test)]
pub(crate) mod live_query_tests;

mod local_foreign_keys;

mod schema_tests;

#[cfg(test)]
mod release_medium_tests;

#[cfg(test)]
mod release_list_rows_tests;

#[cfg(test)]
pub(crate) mod window_cost;
