// Fixture row ids. coven validates every synced row's primary key as a
// canonical v4 UUID (`RowIdentity::IndependentUuid`), which is what bae's
// real ids are, so these fixtures carry UUIDs too. Each constant is named
// for the moniker it replaced, so assertions still read by name.
const AA_SHARED: &str = "a98ba9aa-a32b-4716-842f-5505dee028f0"; // was "aa-shared"
const ALBUM_1: &str = "9644b84d-94b2-4b3b-863a-d6583931920c"; // was "9fd7bfa8-3c7c-4026-8559-da66af02f636"
const ALBUM_1999: &str = "88f57246-3e65-4eb9-8d36-ee8d40326cfc"; // was "album-1999"
const ALBUM_2001_LOWER: &str = "88183677-683b-485e-8224-f6a328c233c7"; // was "album-2001-lower"
const ALBUM_2001_UPPER: &str = "a663cff7-fad7-45b1-8469-5f77af82ddb8"; // was "album-2001-upper"
const ALBUM_A: &str = "a67c03ad-425f-45e9-8279-0144c852aaa5"; // was "album-a"
const ALBUM_ARTIST_1: &str = "288a78d6-b93d-4b4e-8452-fb678e33c2e8"; // was "album-artist-1"
const ALBUM_JUNCTION: &str = "7e6f42e7-8952-48e6-89bf-d1bcc611176d"; // was "album-junction"
const ALBUM_NEW: &str = "7d40ec33-80aa-4ab5-8010-78b55943ad81"; // was "album-new"
const ALBUM_NULL: &str = "c6648d5a-617e-4b69-87da-b7f1c4fb5e65"; // was "album-null"
const ALBUM_OLD: &str = "d80af162-0f69-4558-803e-742f4089d486"; // was "album-old"
const ALBUM_PERCENT: &str = "7e9948c4-f2d0-4a73-8e5c-a885eda086ff"; // was "album-percent"
const ALBUM_PRIOR: &str = "05d41bd9-ace6-4c2e-832e-0ef657f0caf3"; // was "album-prior"
const ALBUM_UNDERSCORE: &str = "2dd55a3f-3208-4faf-8737-453f474074cb"; // was "album-underscore"
const ARTIST_1: &str = "6c441836-aef7-4239-8a84-5336c4cce52c"; // was "artist-1"
const ARTIST_A: &str = "d7d8141f-54ff-467d-8b60-4f34a4d2e528"; // was "artist-a"
const ARTIST_ABSENT: &str = "78420eae-1cd1-4a36-87ae-2a5556aa52aa"; // was "artist-absent"
const ARTIST_ALBUM: &str = "85f70840-aba5-4eb9-8e1a-0d319e53b798"; // was "artist-album"
const ARTIST_B: &str = "38fc314c-c130-4120-8ca9-38b870ccef3a"; // was "artist-b"
const ARTIST_C: &str = "1b4bafc9-0ece-4538-833e-4ff52feb6ef0"; // was "artist-c"
const ARTIST_COMPOSER: &str = "5412b7ad-bdc1-4561-8985-b6d6ef8a2880"; // was "artist-composer"
const ARTIST_EXCLUSIVE: &str = "529eb0a5-b0bd-4e28-8c21-77fe62f8c77d"; // was "artist-exclusive"
const ARTIST_EXTRA: &str = "7fa00099-f5d8-4ec2-88bd-e19d8edd7bb8"; // was "artist-extra"
const ARTIST_PRIMARY: &str = "7cdf9a34-0746-472b-8c68-0a669c11f2f1"; // was "artist-primary"
const ARTIST_SHARED: &str = "44d4b0bf-fd8a-4145-8deb-aa676bb4212a"; // was "artist-shared"
const ARTIST_SOLO: &str = "49549823-0e72-4747-891e-ee50e1611e3a"; // was "artist-solo"
const ARTIST_VARIOUS: &str = "f862abf2-3b15-4518-889b-1996d7100201"; // was "artist-various"
const ARTIST_WORK_ONLY: &str = "b96d8066-777d-408d-8ae4-ed58c767e40c"; // was "artist-work-only"
const BLOB_1: &str = "222d362a-5ce1-45ff-8a54-341cde525c2c"; // was "blob-1"
const BLOB_2: &str = "b1b46178-280d-48d4-86b3-62b31c040179"; // was "blob-2"
const COMPOSER_A: &str = "5dcc4999-03bd-42cc-8d14-8bf0a05effa3"; // was "composer-a"
const COMPOSER_B: &str = "2b748d47-e5b7-4c40-8716-1e608b9dfc3d"; // was "composer-b"
const COMPOSER_C: &str = "80cd3a5e-7fb7-4766-8ec3-d8e86575743b"; // was "composer-c"
const COMPOSER_SOLO: &str = "4d93d615-4549-45d9-81d9-644f079d59bf"; // was "composer-solo"
const ENTRY_A: &str = "e2ebff4e-4ed0-4a73-88ed-93453a79b463"; // was "entry-a"
const FILE_A: &str = "c9a20987-a1bf-4afe-890e-635c6cc13363"; // was "file-a"
const FILE_NEW: &str = "48804352-31c6-4a7c-8f44-9ac4cc62abdf"; // was "file-new"
const REL_1: &str = "cccb6034-5922-40d2-8d0b-d94619230882"; // was "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e"
const REL_A: &str = "25b35e24-5ff2-45e7-88ca-dc3e06995053"; // was "rel-a"
const REL_B: &str = "cee9702c-4399-45e8-894e-34aa16788938"; // was "rel-b"
const REL_NEW: &str = "f3078482-3f35-4019-8ade-a04971532682"; // was "rel-new"
const REL_OLD: &str = "3113dc59-d689-4c8c-86e9-4a3ae1565563"; // was "rel-old"
const REL_ONE: &str = "35fa3546-ff78-4214-857a-d323014e4e2c"; // was "rel-one"
const REL_TWO: &str = "6f389f38-00da-41c6-8dbf-365b1f7823fe"; // was "rel-two"
const RELEASE_1: &str = "c0218676-4c47-4eb7-8d65-57a8d328c3d1"; // was "release-1"
const RELEASE_A: &str = "0252dedb-ee39-4547-8803-438dbeb57a64"; // was "release-a"
const RELEASE_B: &str = "64e79a1f-404a-4c34-809a-a3cb44bf1942"; // was "release-b"
const RELEASE_LONELY: &str = "fcf4be32-159f-4790-87a1-697700a74462"; // was "release-lonely"
const RELEASE_OTHER: &str = "ce596bd7-be97-4416-8b6d-47f315bae466"; // was "release-other"
const RELEASE_PRIOR: &str = "878449fa-3b87-44f5-8e6b-5af3d41ea386"; // was "release-prior"
const RELEASE_ROLE_A: &str = "9b72bbbf-621e-41ca-8930-1623b643a20d"; // was "release-role-a"
const RELEASE_Z: &str = "8aa66d48-65a0-42e4-8c1d-e7481e8c1861"; // was "release-z"
const TRACK_A: &str = "0482872e-d4bf-4080-8426-441a0a3e71fc"; // was "track-a"
const TRACK_B: &str = "04676261-1659-47b1-879c-2947c52f4a8d"; // was "track-b"
const TRACK_LONELY: &str = "03c41035-ce18-4fa0-8e83-c446df26a551"; // was "track-lonely"
const TRACK_NEW: &str = "d28100a4-a355-47d3-8d5d-5a7b80bc66fd"; // was "track-new"
const TRACK_OTHER: &str = "69e67928-545a-4dcf-8ae7-ef7778331231"; // was "track-other"
const TRACK_PERCENT: &str = "4dc8cde9-15fb-470d-802c-b7e5f1ccc63d"; // was "track-percent"
const TRACK_PRIOR: &str = "6e9ff639-e1b3-48bf-84c5-1cc1794f3f70"; // was "track-prior"
const TRACK_ROLE_A: &str = "fa0c8483-f09a-4b69-8903-b1ebcdc31322"; // was "track-role-a"
const TRACK_UNDERSCORE: &str = "b2930937-dae6-4719-8150-aa61422eeeac"; // was "track-underscore"
const TRACK_WORK_A: &str = "d410a973-6a19-4ad3-87d8-b0c8c13d6015"; // was "track-work-a"
const WORK_A: &str = "432c8996-8af0-43dc-868a-822a256f65c4"; // was "work-a"
const WORK_ARTIST_A: &str = "ec41a8cd-a9a4-473e-8b70-d78168aefd8e"; // was "work-artist-a"
const WORK_CHILD_A: &str = "f63d8e66-6a81-4a67-8005-1fbe870f27eb"; // was "work-child-a"
const WORK_PARENT_A: &str = "6b05af7a-ee0c-4f12-8938-1d5536697271"; // was "work-parent-a"

#[cfg(test)]
mod queue_ordering_tests {
    use super::super::*;
    use crate::playback::QueueEntryId;
    use std::collections::HashMap;

    fn meta(id: &str) -> TrackQueueMeta {
        TrackQueueMeta {
            title: format!("Title {id}"),
            artist_names: "Artist Name".to_string(),
            duration_ms: Some(1000),
            album_title: "Album Title".to_string(),
            cover_image: Some(crate::album_detail::ImageRef {
                id: format!("rel-{id}"),
                version: format!("stamp-{id}"),
                image_type: LibraryImageType::Cover,
            }),
        }
    }

    fn entry(entry_id: &str, track_id: &str) -> QueueEntry {
        QueueEntry {
            id: QueueEntryId(entry_id.to_string()),
            track_id: track_id.to_string(),
        }
    }

    #[test]
    fn preserves_duplicate_queue_entries_in_order_with_distinct_ids() {
        let mut meta_by_track = HashMap::new();
        meta_by_track.insert("a".to_string(), meta("a"));
        meta_by_track.insert("b".to_string(), meta("b"));

        // The same track queued twice resolves twice, in position order, each
        // carrying its own entry id.
        let resolved = resolve_queue_entries(
            &meta_by_track,
            &[entry("e0", "a"), entry("e1", "a"), entry("e2", "b")],
        );

        let track_ids: Vec<&str> = resolved.iter().map(|i| i.track_id.as_str()).collect();
        assert_eq!(track_ids, vec!["a", "a", "b"]);
        let entry_ids: Vec<&str> = resolved.iter().map(|i| i.entry_id.as_str()).collect();
        assert_eq!(entry_ids, vec!["e0", "e1", "e2"]);
    }

    #[test]
    fn skips_entries_whose_track_is_unknown() {
        let mut meta_by_track = HashMap::new();
        meta_by_track.insert("a".to_string(), meta("a"));
        let resolved =
            resolve_queue_entries(&meta_by_track, &[entry("e0", "a"), entry("e1", "missing")]);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].entry_id, "e0");
    }
}

#[cfg(test)]
mod in_clause_chunking_tests {
    use super::super::*;
    use super::*;
    use crate::playback::QueueEntryId;
    use coven::SystemClock;
    use std::sync::Arc;

    async fn chunked_track_db() -> (Database, tempfile::TempDir, Vec<String>) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let track_count = SQL_MAX_IN_VARS * 45;
        let track_ids: Vec<String> = (0..track_count)
            .map(|index| bae_test_support::test_uuid(&format!("track-{index}")))
            .collect();
        let seed_track_ids = track_ids.clone();
        db.call(move |conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, _updated_at, created_at)
                VALUES ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, '0252dedb-ee39-4547-8803-438dbeb57a64', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');
                ",
            )?;
            // The ids are minted in Rust, not in SQL: coven takes only canonical
            // UUIDs on a synced row, and the test's assertions need the same
            // values the seed wrote.
            for (index, track_id) in seed_track_ids.iter().enumerate() {
                conn.execute(
                    "INSERT INTO tracks \
                         (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at) \
                     VALUES (?1, '0252dedb-ee39-4547-8803-438dbeb57a64', ?2, 1, ?3, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z')",
                    params![track_id, format!("Track Title {index}"), index as i64],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
        (db, tmp, track_ids)
    }

    /// `cover_versions` takes a whole page's release ids, which is unbounded, so it
    /// chunks like its siblings. Past SQLite's variable limit an unchunked `IN`
    /// doesn't return fewer rows — it fails the query outright.
    #[tokio::test]
    async fn cover_versions_merges_chunks() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();

        let cover_count = SQL_MAX_IN_VARS * 3;
        // Minted in Rust for the same reason as the track seed: coven takes only
        // canonical UUIDs on a synced row.
        let release_ids: Vec<String> = (0..cover_count)
            .map(|i| bae_test_support::test_uuid(&format!("release-{i}")))
            .collect();
        let seed_release_ids = release_ids.clone();
        db.call(move |conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, _updated_at, created_at)
                VALUES ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, 'cdb9e2f2-ba4c-43ac-8422-765445141290', 0, 'stamp', '2026-01-01T00:00:00Z');
                ",
            )?;
            // covers.id is a FK to releases.id, so every cover needs its release.
            for (index, release_id) in seed_release_ids.iter().enumerate() {
                conn.execute(
                    "INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
                     VALUES (?1, 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z')",
                    params![release_id],
                )?;
                conn.execute(
                    "INSERT INTO covers \
                         (id, blob_id, content_type, file_size, source, hash, _updated_at, created_at) \
                     VALUES (?1, ?2, 'image/jpeg', 1024, 'discogs', ?3, ?4, '2026-01-01T00:00:00Z')",
                    params![
                        release_id,
                        bae_test_support::test_uuid(&format!("cover-blob-{index}")),
                        crate::util::fs::hash_bytes(b"fixture"),
                        format!("stamp-{index}")
                    ],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();

        let versions = db.cover_versions(&release_ids).await.unwrap();

        // Spans three chunks; every row from every chunk must survive the merge.
        assert_eq!(versions.len(), cover_count);
        assert_eq!(
            versions.get(&release_ids[0]).map(String::as_str),
            Some("stamp-0")
        );
        let last = cover_count - 1;
        assert_eq!(
            versions.get(&release_ids[last]).map(String::as_str),
            Some(format!("stamp-{last}").as_str())
        );
    }

    #[tokio::test]
    async fn track_id_queries_merge_chunks() {
        let (db, _tmp, track_ids) = chunked_track_db().await;
        let mut requested = track_ids.clone();
        requested.insert(SQL_MAX_IN_VARS / 2, "missing-track".to_string());

        let mut existing = db.filter_existing_track_ids(&requested).await.unwrap();
        existing.sort();
        let mut expected_existing = track_ids.clone();
        expected_existing.sort();
        assert_eq!(existing, expected_existing);

        let album_ids = db.get_album_ids_for_tracks(&requested).await.unwrap();
        assert_eq!(album_ids.len(), track_ids.len());
        for track_id in &track_ids {
            assert_eq!(album_ids.get(track_id).map(String::as_str), Some(ALBUM_A));
        }

        let entries: Vec<QueueEntry> = requested
            .iter()
            .enumerate()
            .map(|(index, track_id)| QueueEntry {
                id: QueueEntryId(format!("entry-{index}")),
                track_id: track_id.clone(),
            })
            .collect();
        let items = db.get_queue_items(&entries).await.unwrap();
        let resolved_track_ids: Vec<&str> =
            items.iter().map(|item| item.track_id.as_str()).collect();
        let expected_track_ids: Vec<&str> = requested
            .iter()
            .filter(|track_id| track_id.as_str() != "missing-track")
            .map(String::as_str)
            .collect();
        assert_eq!(resolved_track_ids, expected_track_ids);
    }
}

#[cfg(test)]
mod aggregate_ordering_tests {
    use super::super::*;
    use super::*;
    use crate::playback::QueueEntryId;
    use coven::SystemClock;
    use std::sync::Arc;

    async fn aggregate_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                PRAGMA reverse_unordered_selects = ON;

                INSERT INTO artists (id, name, _updated_at, created_at)
                VALUES
                    ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z'),
                    ('7e7d8df5-8292-4287-80be-7abd24f5a992', 'Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                    ('863911c6-a6b6-40a7-8096-b85eb877f7c7', 'Artist Name Second', 'stamp', '2026-01-01T00:00:00Z'),
                    ('c0770501-2551-4e87-8801-93f780248cf3', 'Composer Name First', 'stamp', '2026-01-01T00:00:00Z'),
                    ('0d0e8916-becd-4d2c-89e0-7cc5c7005f83', 'Composer Name Second', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, NULL, 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
                VALUES
                    ('2bd047a8-0ed8-4f71-851d-e168c16cbd36', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '863911c6-a6b6-40a7-8096-b85eb877f7c7', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('1c2ab709-4221-4c38-8d0c-ff1d18107cce', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '7e7d8df5-8292-4287-80be-7abd24f5a992', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('8aa66d48-65a0-42e4-8c1d-e7481e8c1861', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('64e79a1f-404a-4c34-809a-a3cb44bf1942', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:01Z'),
                    ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:01Z');

                INSERT INTO works (id, title, work_type, _updated_at, created_at)
                VALUES ('432c8996-8af0-43dc-868a-822a256f65c4', 'Work Title A', 'work', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
                VALUES
                    ('ba0d989a-7cb6-4050-8247-9a5424b33041', '432c8996-8af0-43dc-868a-822a256f65c4', '0d0e8916-becd-4d2c-89e0-7cc5c7005f83', 1, 'file_tags', 'stamp', '2026-01-01T00:00:00Z'),
                    ('6027384b-545d-4289-8123-7201ec25276f', '432c8996-8af0-43dc-868a-822a256f65c4', 'c0770501-2551-4e87-8801-93f780248cf3', 0, 'file_tags', 'stamp', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();
        (db, tmp)
    }

    #[tokio::test]
    async fn album_summary_orders_artist_names_and_release_ids_inside_aggregates() {
        let (db, _tmp) = aggregate_db().await;
        let summary = db
            .find_album_summary(ALBUM_A)
            .await
            .unwrap()
            .expect("album summary row");

        assert_eq!(
            summary.artist_names,
            "Artist Name Primary, Artist Name First, Artist Name Second"
        );
        assert_eq!(summary.release_ids, vec![RELEASE_Z, RELEASE_A, RELEASE_B]);
    }

    #[tokio::test]
    async fn work_summary_orders_composer_names_inside_the_aggregate() {
        let (db, _tmp) = aggregate_db().await;
        let results = db.search_library("Work Title A", 10).await.unwrap();
        let summary = results.works.first().expect("work summary row");

        assert_eq!(
            summary.composer_names.as_deref(),
            Some("Composer Name First, Composer Name Second")
        );
    }

    #[tokio::test]
    async fn release_storage_summary_orders_artist_names_inside_the_aggregate() {
        let (db, _tmp) = aggregate_db().await;
        let summary = db
            .find_release_storage_summary(RELEASE_Z)
            .await
            .unwrap()
            .expect("release storage summary row");

        assert_eq!(
            summary.artist_names,
            "Artist Name Primary, Artist Name First, Artist Name Second"
        );
    }

    #[tokio::test]
    async fn storage_page_orders_album_aggregate_columns_inside_aggregates() {
        let (db, _tmp) = aggregate_db().await;
        let sort = StorageSortCriterion {
            field: StorageSortField::AlbumTitle,
            direction: SortDirection::Ascending,
        };
        let rows = db
            .get_storage_page(&sort, StorageFilter::All, 0, 10)
            .await
            .unwrap();
        let row = rows.first().expect("storage row");

        assert_eq!(
            row.album.artist_names,
            "Artist Name Primary, Artist Name First, Artist Name Second"
        );
        assert_eq!(row.album.release_ids, vec![RELEASE_Z, RELEASE_A, RELEASE_B]);
    }

    #[tokio::test]
    async fn album_and_storage_pages_allow_missing_primary_artist() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, _updated_at, created_at)
                VALUES
                    ('7e7d8df5-8292-4287-80be-7abd24f5a992', 'Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                    ('863911c6-a6b6-40a7-8096-b85eb877f7c7', 'Artist Name Second', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('f6506bc5-0b41-44de-862f-1668e72c08c6', 'Album Title Empty', NULL, 2026, '7c3d0881-e6d0-4252-8075-709b2282bcc1', 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('049faa5b-52d9-4109-832b-f6853740c876', 'Album Title Extra', NULL, 2026, 'ba00ebe0-da50-428a-8ceb-2389d9a9f232', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
                VALUES
                    ('c9966a2c-61b1-40dd-87ef-118587e57fe7', '049faa5b-52d9-4109-832b-f6853740c876', '863911c6-a6b6-40a7-8096-b85eb877f7c7', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('4953a209-c6c7-4b0c-82dd-cc13e47af890', '049faa5b-52d9-4109-832b-f6853740c876', '7e7d8df5-8292-4287-80be-7abd24f5a992', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('7c3d0881-e6d0-4252-8075-709b2282bcc1', 'f6506bc5-0b41-44de-862f-1668e72c08c6', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('ba00ebe0-da50-428a-8ceb-2389d9a9f232', '049faa5b-52d9-4109-832b-f6853740c876', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let album_sort = [AlbumSortCriterion {
            field: AlbumSortField::Title,
            direction: SortDirection::Ascending,
        }];
        let albums = db.get_album_page(&album_sort, 0, 10).await.unwrap();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].artist_names, "");
        assert_eq!(
            albums[1].artist_names,
            "Artist Name First, Artist Name Second"
        );

        let storage_sort = StorageSortCriterion {
            field: StorageSortField::AlbumTitle,
            direction: SortDirection::Ascending,
        };
        let rows = db
            .get_storage_page(&storage_sort, StorageFilter::All, 0, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].album.artist_names, "");
        assert_eq!(
            rows[1].album.artist_names,
            "Artist Name First, Artist Name Second"
        );
    }

    async fn queue_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                PRAGMA reverse_unordered_selects = ON;

                INSERT INTO artists (id, name, _updated_at, created_at)
                VALUES
                    ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z'),
                    ('5b5f8c38-5237-4187-895c-28b1b2a43672', 'Track Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                    ('8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 'Track Artist Name Second', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, '0252dedb-ee39-4547-8803-438dbeb57a64', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
                VALUES ('0482872e-d4bf-4080-8426-441a0a3e71fc', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title A', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO track_artists (id, track_id, artist_id, position, _updated_at, created_at)
                VALUES
                    ('af940e5f-472b-4162-81fb-97517afd23be', '0482872e-d4bf-4080-8426-441a0a3e71fc', '8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('8b08019f-1c04-400e-8107-9b85f7222407', '0482872e-d4bf-4080-8426-441a0a3e71fc', '5b5f8c38-5237-4187-895c-28b1b2a43672', 0, 'stamp', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();
        (db, tmp)
    }

    #[tokio::test]
    async fn queue_items_order_track_artist_names_inside_the_aggregate() {
        let (db, _tmp) = queue_db().await;
        let items = db
            .get_queue_items(&[QueueEntry {
                id: QueueEntryId(ENTRY_A.to_string()),
                track_id: TRACK_A.to_string(),
            }])
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].artist_names,
            "Track Artist Name First, Track Artist Name Second"
        );
    }

    async fn release_detail_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                PRAGMA reverse_unordered_selects = ON;

                INSERT INTO artists (id, name, _updated_at, created_at)
                VALUES
                    ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z'),
                    ('5b5f8c38-5237-4187-895c-28b1b2a43672', 'Track Artist Name First', 'stamp', '2026-01-01T00:00:00Z'),
                    ('8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 'Track Artist Name Second', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, '0252dedb-ee39-4547-8803-438dbeb57a64', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
                VALUES
                    ('04676261-1659-47b1-879c-2947c52f4a8d', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title B', 1, 2, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('0482872e-d4bf-4080-8426-441a0a3e71fc', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title A', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO track_artists (id, track_id, artist_id, position, _updated_at, created_at)
                VALUES
                    ('af940e5f-472b-4162-81fb-97517afd23be', '0482872e-d4bf-4080-8426-441a0a3e71fc', '8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('8b08019f-1c04-400e-8107-9b85f7222407', '0482872e-d4bf-4080-8426-441a0a3e71fc', '5b5f8c38-5237-4187-895c-28b1b2a43672', 0, 'stamp', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();
        (db, tmp)
    }

    #[tokio::test]
    async fn release_detail_orders_track_artists_and_keeps_tracks_without_artists() {
        let (db, _tmp) = release_detail_db().await;
        let detail = db
            .find_release_detail(RELEASE_A)
            .await
            .unwrap()
            .expect("release detail");

        let track_ids: Vec<&str> = detail
            .tracks
            .iter()
            .map(|track| track.track.id.as_str())
            .collect();
        assert_eq!(track_ids, vec![TRACK_A, TRACK_B]);

        let artist_names: Vec<&str> = detail.tracks[0]
            .artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect();
        assert_eq!(
            artist_names,
            vec!["Track Artist Name First", "Track Artist Name Second"]
        );
        assert!(detail.tracks[1].artists.is_empty());
    }
}

#[cfg(test)]
mod connection_boundary_tests {
    use super::super::*;
    use super::*;
    use coven::SystemClock;
    use std::sync::Arc;

    #[tokio::test]
    async fn coven_connection_enforces_foreign_keys_for_bae_schema() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();

        let track = DbTrack::new_test("missing-release", TRACK_A, "Track Title A", Some(1));
        let error = db
            .insert_track(&track)
            .await
            .expect_err("track insert without a release must violate the foreign key");

        assert!(
            error.to_string().contains("FOREIGN KEY constraint failed"),
            "expected a foreign-key violation, got {error}"
        );
    }
}

#[cfg(test)]
mod readable_cloud_path_tests {
    use super::super::*;
    use super::*;

    /// An in-memory DB on the real schema with one artist/album/release, so the
    /// connection-level resolvers can look up a release's album id.
    fn seeded_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../../../migrations/001_initial.sql"))
            .unwrap();
        let now = "2026-01-01T00:00:00Z";
        conn.execute(
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('6c441836-aef7-4239-8a84-5336c4cce52c', 'Artist Name', ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) \
             VALUES ('9644b84d-94b2-4b3b-863a-d6583931920c', 'Album Title', '6c441836-aef7-4239-8a84-5336c4cce52c', 0, ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
             VALUES ('cccb6034-5922-40d2-8d0b-d94619230882', '9644b84d-94b2-4b3b-863a-d6583931920c', 'file_tags', 1, ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn
    }

    #[test]
    fn audio_key_omits_source_folder_when_release_has_none() {
        // The seeded release has no source_folder_name (a non-folder import). The
        // stored key is namespace-relative; coven prepends the `release_files`
        // namespace when it reads/writes the blob.
        let conn = seeded_conn();
        let key = resolve_audio_cloud_path(&conn, REL_1, "01 Track Title.flac").unwrap();
        assert_eq!(key, format!("{ALBUM_1}/{REL_1}/01 Track Title.flac"));
    }

    #[test]
    fn audio_key_includes_source_folder_from_the_release_row() {
        let conn = seeded_conn();
        conn.execute(
            "UPDATE releases SET source_folder_name = 'Album Folder [FLAC]' WHERE id = 'cccb6034-5922-40d2-8d0b-d94619230882'",
            [],
        )
        .unwrap();
        let key = resolve_audio_cloud_path(&conn, REL_1, "01 Track Title.flac").unwrap();
        assert_eq!(
            key,
            format!("{ALBUM_1}/{REL_1}/Album Folder [FLAC]/01 Track Title.flac")
        );
    }

    #[test]
    fn cover_key_is_album_release_and_blob_id() {
        // The blob id rides in the key, so a replaced cover writes a new object
        // rather than overwriting the one it replaces.
        let conn = seeded_conn();
        let key = resolve_cover_cloud_path(&conn, REL_1, BLOB_1, &ContentType::Jpeg).unwrap();
        assert_eq!(key, format!("{ALBUM_1}/{REL_1}/cover-{BLOB_1}.jpg"));
        let replaced = resolve_cover_cloud_path(&conn, REL_1, BLOB_2, &ContentType::Jpeg).unwrap();
        assert_ne!(key, replaced);
    }

    #[test]
    fn artist_key_is_artist_and_blob_id() {
        // Keyed by the artist and its blob id alone -- no DB lookup.
        let key = resolve_artist_cloud_path(ARTIST_1, BLOB_1, &ContentType::Png);
        assert_eq!(key, format!("{ARTIST_1}/artist-{BLOB_1}.png"));
    }

    #[test]
    fn missing_release_is_an_error() {
        // The release row must exist when a blob is keyed; its absence is a
        // broken invariant surfaced as an error, not masked.
        let conn = seeded_conn();
        assert!(resolve_audio_cloud_path(&conn, "no-such-release", "x.flac").is_err());
    }
}

#[cfg(test)]
mod row_mapper_error_tests {
    use super::super::*;

    /// An in-memory DB on the real schema with one artist/album/release whose
    /// `created_at`/`metadata_source` are valid, so a test can corrupt one
    /// column and prove the mapper rejects it.
    fn seeded_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../../../migrations/001_initial.sql"))
            .unwrap();
        let now = "2026-01-01T00:00:00Z";
        conn.execute(
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('6c441836-aef7-4239-8a84-5336c4cce52c', 'Artist Name', ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) \
             VALUES ('9644b84d-94b2-4b3b-863a-d6583931920c', 'Album Title', '6c441836-aef7-4239-8a84-5336c4cce52c', 0, ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
             VALUES ('cccb6034-5922-40d2-8d0b-d94619230882', '9644b84d-94b2-4b3b-863a-d6583931920c', 'file_tags', 1, ?, ?)",
            params![now, now],
        )
        .unwrap();
        conn
    }

    #[test]
    fn row_to_release_rejects_malformed_created_at() {
        // A corrupt timestamp must propagate as an error, not panic the mapper.
        let conn = seeded_conn();
        conn.execute(
            "UPDATE releases SET created_at = 'not-a-timestamp' WHERE id = 'cccb6034-5922-40d2-8d0b-d94619230882'",
            [],
        )
        .unwrap();
        let result = conn.query_row(
            "SELECT * FROM releases WHERE id = 'cccb6034-5922-40d2-8d0b-d94619230882'",
            [],
            row_to_release,
        );
        assert!(result.is_err());
    }

    #[test]
    fn row_to_release_rejects_unknown_metadata_source() {
        // An unknown enum string must propagate, not panic via expect.
        let conn = seeded_conn();
        conn.execute(
            "UPDATE releases SET metadata_source = 'bogus' WHERE id = 'cccb6034-5922-40d2-8d0b-d94619230882'",
            [],
        )
        .unwrap();
        let result = conn.query_row(
            "SELECT * FROM releases WHERE id = 'cccb6034-5922-40d2-8d0b-d94619230882'",
            [],
            row_to_release,
        );
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod composer_mode_tests {
    use super::super::*;
    use super::*;
    use coven::SystemClock;

    /// A replacement with no blob/outbox cleanup — the released-in-place local
    /// files these album-cleanup tests use carry no cloud state to tear down.
    fn empty_cleanup_plan() -> DeleteCleanupPlan {
        DeleteCleanupPlan::default()
    }

    /// Shared arrange for the two reimport-replacement tests. Seeds `album-old` with
    /// `existing_release_ids` and its `primary_release_id` at `replaced_release_id`,
    /// then finalizes a reimport whose new release `rel-new` lands in the fresh
    /// `album-new`, carrying an `ImportReplacementDelete` for `replaced_release_id`.
    /// Returns the finalize outcomes, so each test asserts the album's fate itself.
    async fn finalize_reimport_replacing_release(
        db: &Database,
        tmp: &tempfile::TempDir,
        now: chrono::DateTime<chrono::Utc>,
        existing_release_ids: &[&str],
        replaced_release_id: &str,
    ) -> Vec<ImportReplacementOutcome> {
        let artist = DbArtist {
            id: ARTIST_A.to_string(),
            name: "Artist Name A".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        db.insert_artist(&artist).await.unwrap();

        let album_old = DbAlbum {
            id: ALBUM_OLD.to_string(),
            title: "Album Title Old".to_string(),
            artist_id: artist.id.clone(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        db.insert_album(&album_old).await.unwrap();
        for id in existing_release_ids {
            db.insert_release(&DbRelease::new_test(&album_old.id, id))
                .await
                .unwrap();
        }
        db.set_album_primary_release(&album_old.id, replaced_release_id)
            .await
            .unwrap();

        let album_new = DbAlbum {
            id: ALBUM_NEW.to_string(),
            title: "Album Title New".to_string(),
            artist_id: artist.id.clone(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release_new = DbRelease::new_test(&album_new.id, REL_NEW);
        let track = DbTrack {
            id: TRACK_NEW.to_string(),
            release_id: release_new.id.clone(),
            title: "Track Title New".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(1000),
            discogs_position: None,
            created_at: now,
        };
        let file = DbFile::new(
            &release_new.id,
            "Track Title New.flac",
            1024,
            ContentType::Flac,
            FILE_NEW.to_string(),
            now,
            crate::util::fs::hash_bytes(b"fixture"),
        );
        let track_files = vec![crate::import::TrackFile::Standalone {
            db_track: track,
            file_path: tmp.path().join("Track Title New.flac"),
        }];

        let replacement = ImportReplacementDelete {
            release_id: replaced_release_id.to_string(),
            album_id: album_old.id.clone(),
            cleanup: empty_cleanup_plan(),
        };
        db.finalize_import_atomic(
            Some(&album_new),
            &release_new,
            &track_files,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[file],
            &[],
            &[],
            None,
            &[],
            None,
            &[],
            tmp.path().to_str().unwrap(),
            crate::config::HomeStorage::Opaque,
            &[replacement],
        )
        .await
        .unwrap()
    }

    async fn seeded_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('85f70840-aba5-4eb9-8e1a-0d319e53b798', 'Album Artist A', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('5412b7ad-bdc1-4561-8985-b6d6ef8a2880', 'Displayed Composer A', 'Hidden Composer Sort A', NULL, 'mb-artist-composer-a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '85f70840-aba5-4eb9-8e1a-0d319e53b798', 2026, '0252dedb-ee39-4547-8803-438dbeb57a64', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, release_name, year, disc_id, metadata_source, metadata_source_release_id, format, label, catalog_number, country, barcode, remote, source_folder_name, content_hash, album_loudness_lufs, album_peak_linear, _updated_at, created_at)
                VALUES ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', NULL, 2026, NULL, 'musicbrainz', 'mb-release-a', 'CD', NULL, NULL, NULL, NULL, 1, NULL, NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
                VALUES ('0482872e-d4bf-4080-8426-441a0a3e71fc', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title A', 1, 1, 1000, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO works (id, title, disambiguation, work_type, _updated_at, created_at)
                VALUES
                    ('6b05af7a-ee0c-4f12-8938-1d5536697271', 'Parent Work A', NULL, 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('f63d8e66-6a81-4a67-8005-1fbe870f27eb', 'Displayed Work A', NULL, 'part', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
                VALUES ('ec41a8cd-a9a4-473e-8b70-d78168aefd8e', 'f63d8e66-6a81-4a67-8005-1fbe870f27eb', '5412b7ad-bdc1-4561-8985-b6d6ef8a2880', 0, 'musicbrainz', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO work_parts (id, parent_work_id, child_work_id, position, source, _updated_at, created_at)
                VALUES ('fa383452-6671-4335-87bc-751a52bbdde5', '6b05af7a-ee0c-4f12-8938-1d5536697271', 'f63d8e66-6a81-4a67-8005-1fbe870f27eb', 0, 'musicbrainz', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO track_works (id, track_id, work_id, position, source, _updated_at, created_at)
                VALUES ('d410a973-6a19-4ad3-87d8-b0c8c13d6015', '0482872e-d4bf-4080-8426-441a0a3e71fc', 'f63d8e66-6a81-4a67-8005-1fbe870f27eb', 0, 'musicbrainz', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();
        (db, tmp)
    }

    #[tokio::test]
    async fn finalize_import_persists_composer_work_and_role_rows() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let album_artist = DbArtist {
            id: ARTIST_ALBUM.to_string(),
            name: "Album Artist A".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        let composer = DbArtist {
            id: ARTIST_COMPOSER.to_string(),
            name: "Composer Artist A".to_string(),
            sort_name: Some("Composer Artist A".to_string()),
            discogs_artist_id: None,
            musicbrainz_artist_id: Some("mb-artist-composer-a".to_string()),
            created_at: now,
        };
        db.insert_artist(&album_artist).await.unwrap();
        db.insert_artist(&composer).await.unwrap();

        let album = DbAlbum {
            id: ALBUM_A.to_string(),
            title: "Album Title A".to_string(),
            artist_id: album_artist.id.clone(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release = DbRelease {
            id: RELEASE_A.to_string(),
            album_id: album.id.clone(),
            release_name: None,
            pressing: Pressing {
                year: Some(2026),
                format: Some("CD".to_string()),
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
            },
            disc_id: None,
            metadata_source: ReleaseMetadataSource::MusicBrainz,
            metadata_source_release_id: Some("mb-release-a".to_string()),
            remote: true,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        };
        let track = DbTrack {
            id: TRACK_A.to_string(),
            release_id: release.id.clone(),
            title: "Track Title A".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(1000),
            discogs_position: None,
            created_at: now,
        };
        let track_files = vec![crate::import::TrackFile::Standalone {
            db_track: track,
            file_path: tmp.path().join("Track.flac"),
        }];
        let works = vec![DbWork::new(
            WORK_A,
            "Work Title A",
            None,
            Some("work".to_string()),
            now,
        )];
        let work_artists = vec![DbWorkArtist::new(
            WORK_A,
            &composer.id,
            0,
            crate::import::MetadataSource::MusicBrainz,
            WORK_ARTIST_A.to_string(),
            now,
        )];
        let track_works = vec![DbTrackWork::new(
            TRACK_A,
            WORK_A,
            0,
            crate::import::MetadataSource::MusicBrainz,
            TRACK_WORK_A.to_string(),
            now,
        )];
        let release_roles = vec![DbReleaseArtistRole::new(
            &release.id,
            &composer.id,
            0,
            crate::import::MetadataSource::Discogs,
            Some("Conducted By".to_string()),
            RELEASE_ROLE_A.to_string(),
            now,
        )];
        let track_roles = vec![DbTrackArtistRole::new(
            TRACK_A,
            &composer.id,
            0,
            crate::import::MetadataSource::MusicBrainz,
            Some("arranger".to_string()),
            TRACK_ROLE_A.to_string(),
            now,
        )];

        db.finalize_import_atomic(
            Some(&album),
            &release,
            &track_files,
            &[],
            &[],
            &works,
            &work_artists,
            &[],
            &track_works,
            &release_roles,
            &track_roles,
            &[],
            &[],
            &[],
            &[],
            &[],
            None,
            &[],
            Some((&album.id, &release.id)),
            &[],
            tmp.path().to_str().unwrap(),
            crate::config::HomeStorage::Opaque,
            &[],
        )
        .await
        .unwrap();

        let composer_detail = db
            .find_composer_detail(&composer.id)
            .await
            .unwrap()
            .expect("composer detail");
        assert_eq!(composer_detail.work_groups.len(), 1);
        assert_eq!(composer_detail.work_groups[0].works[0].work.id, WORK_A);
        let release_role_count = db
            .call(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM release_artist_roles WHERE id = '9b72bbbf-621e-41ca-8930-1623b643a20d'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(DbError::from)
            })
            .await
            .unwrap();
        assert_eq!(release_role_count, 1);

        let work_detail = db
            .find_work_detail(WORK_A)
            .await
            .unwrap()
            .expect("work detail");
        assert_eq!(work_detail.tracks.len(), 1);
        let track_role_count = db
            .call(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM track_artist_roles WHERE id = 'fa0c8483-f09a-4b69-8903-b1ebcdc31322'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(DbError::from)
            })
            .await
            .unwrap();
        assert_eq!(track_role_count, 1);
    }

    #[tokio::test]
    async fn fail_import_and_delete_release_removes_finalized_import_state_atomically() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let artist = DbArtist {
            id: ARTIST_A.to_string(),
            name: "Artist Name A".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        db.insert_artist(&artist).await.unwrap();

        let album = DbAlbum {
            id: ALBUM_A.to_string(),
            title: "Album Title A".to_string(),
            artist_id: artist.id.clone(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release = DbRelease {
            id: RELEASE_A.to_string(),
            album_id: album.id.clone(),
            release_name: None,
            pressing: Pressing {
                year: Some(2026),
                format: Some("FLAC".to_string()),
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
            },
            disc_id: None,
            metadata_source: ReleaseMetadataSource::FileTags,
            metadata_source_release_id: None,
            remote: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        };
        let track = DbTrack {
            id: TRACK_A.to_string(),
            release_id: release.id.clone(),
            title: "Track Title A".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(1000),
            discogs_position: None,
            created_at: now,
        };
        let file = DbFile::new(
            &release.id,
            "Track Title A.flac",
            1024,
            ContentType::Flac,
            FILE_A.to_string(),
            now,
            crate::util::fs::hash_bytes(b"fixture"),
        );
        let track_files = vec![crate::import::TrackFile::Standalone {
            db_track: track,
            file_path: tmp.path().join("Track Title A.flac"),
        }];

        db.finalize_import_atomic(
            Some(&album),
            &release,
            &track_files,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[file],
            &[],
            &[],
            None,
            &[],
            Some((&album.id, &release.id)),
            &[],
            tmp.path().to_str().unwrap(),
            crate::config::HomeStorage::Opaque,
            &[],
        )
        .await
        .unwrap();
        assert!(db.external_blob(FILE_A).await.unwrap().is_some());

        db.fail_import_and_delete_release(RELEASE_A).await.unwrap();

        assert!(db.find_release_by_id(RELEASE_A).await.unwrap().is_none());
        assert!(db.find_album_by_id(ALBUM_A).await.unwrap().is_none());
        // The registration was keyed by the `release_files` row, so the row
        // going takes it with it — there is no "is the ref still there?" left to
        // ask once the row is gone.
        assert!(db
            .get_files_for_release(RELEASE_A)
            .await
            .unwrap()
            .is_empty());
    }

    /// Reimport replacing one of several releases in an album: the prior release
    /// leaves, the album survives, and a `primary_release_id` pointing at the
    /// departed release goes NULL — read paths fall back to the first release left.
    #[tokio::test]
    async fn finalize_replacement_in_surviving_album_clears_dangling_primary() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        // album-old holds rel-one (primary) and rel-two; the reimport replaces
        // rel-one, so the album survives on rel-two.
        let outcomes =
            finalize_reimport_replacing_release(&db, &tmp, now, &[REL_ONE, REL_TWO], REL_ONE).await;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].album_id, ALBUM_OLD);
        assert_eq!(outcomes[0].release_id, REL_ONE);
        assert!(!outcomes[0].album_deleted);

        let surviving = db
            .find_album_by_id(ALBUM_OLD)
            .await
            .unwrap()
            .expect("album survives while rel-two remains");
        assert_eq!(surviving.primary_release_id, None);
        assert!(db.find_release_by_id(REL_ONE).await.unwrap().is_none());
        assert!(db.find_release_by_id(REL_TWO).await.unwrap().is_some());
    }

    /// Reimport replacing an album's sole release, landing in a new album: the prior
    /// album empties and is deleted, and the outcome reports that.
    #[tokio::test]
    async fn finalize_replacement_of_last_release_deletes_prior_album() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        // album-old holds only rel-old; replacing it empties and deletes it.
        let outcomes =
            finalize_reimport_replacing_release(&db, &tmp, now, &[REL_OLD], REL_OLD).await;

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].album_id, ALBUM_OLD);
        assert!(outcomes[0].album_deleted);

        assert!(db.find_album_by_id(ALBUM_OLD).await.unwrap().is_none());
        assert!(db.find_release_by_id(REL_OLD).await.unwrap().is_none());
        assert!(db.find_album_by_id(ALBUM_NEW).await.unwrap().is_some());
        assert!(db.find_release_by_id(REL_NEW).await.unwrap().is_some());
    }

    /// Failed-import rollback of one of several releases in an album: the album
    /// survives, a `primary_release_id` pointing at the failed release goes NULL,
    /// and the sibling release is untouched.
    #[tokio::test]
    async fn fail_import_and_delete_release_in_surviving_album_clears_dangling_primary() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let artist = DbArtist {
            id: ARTIST_A.to_string(),
            name: "Artist Name A".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        db.insert_artist(&artist).await.unwrap();

        let album = DbAlbum {
            id: ALBUM_A.to_string(),
            title: "Album Title A".to_string(),
            artist_id: artist.id.clone(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release = DbRelease::new_test(&album.id, REL_A);
        let track = DbTrack {
            id: TRACK_A.to_string(),
            release_id: release.id.clone(),
            title: "Track Title A".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(1000),
            discogs_position: None,
            created_at: now,
        };
        let file = DbFile::new(
            &release.id,
            "Track Title A.flac",
            1024,
            ContentType::Flac,
            FILE_A.to_string(),
            now,
            crate::util::fs::hash_bytes(b"fixture"),
        );
        let track_files = vec![crate::import::TrackFile::Standalone {
            db_track: track,
            file_path: tmp.path().join("Track Title A.flac"),
        }];

        // Finalize the import, pointing the album's primary at the release
        // this import created.
        db.finalize_import_atomic(
            Some(&album),
            &release,
            &track_files,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[file],
            &[],
            &[],
            None,
            &[],
            Some((&album.id, &release.id)),
            &[],
            tmp.path().to_str().unwrap(),
            crate::config::HomeStorage::Opaque,
            &[],
        )
        .await
        .unwrap();

        // A sibling release in the same album keeps it alive through the
        // rollback.
        let sibling = DbRelease::new_test(&album.id, REL_B);
        db.insert_release(&sibling).await.unwrap();

        db.fail_import_and_delete_release(REL_A).await.unwrap();

        let surviving = db
            .find_album_by_id(ALBUM_A)
            .await
            .unwrap()
            .expect("album survives while sibling remains");
        assert_eq!(surviving.primary_release_id, None);
        assert!(db.find_release_by_id(REL_A).await.unwrap().is_none());
        assert!(db.find_release_by_id(REL_B).await.unwrap().is_some());
    }

    /// A failed remote import's cover and artist-image blobs live only in coven's
    /// on-device store, since the release never went remote. The DB transaction drops
    /// their rows but can't reach the blob store, so `fail_import_and_delete_release`
    /// returns the blobs it orphaned for the caller to evict: the cover and each
    /// deleted artist's image, but not the image of an artist a surviving release
    /// still references.
    #[tokio::test]
    async fn fail_import_and_delete_release_returns_orphaned_image_blobs_to_evict() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let artist = |id: &str| DbArtist {
            id: id.to_string(),
            name: id.to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        db.insert_artist(&artist(ARTIST_EXCLUSIVE)).await.unwrap();
        db.insert_artist(&artist(ARTIST_SHARED)).await.unwrap();

        let pressing = || Pressing {
            year: Some(2026),
            format: Some("FLAC".to_string()),
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        };
        let album = |id: &str, artist_id: &str| DbAlbum {
            id: id.to_string(),
            title: id.to_string(),
            artist_id: artist_id.to_string(),
            year: Some(2026),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release = |id: &str, album_id: &str| DbRelease {
            id: id.to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            pressing: pressing(),
            disc_id: None,
            metadata_source: ReleaseMetadataSource::FileTags,
            metadata_source_release_id: None,
            remote: false,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        };
        let track = |id: &str, release_id: &str| DbTrack {
            id: id.to_string(),
            release_id: release_id.to_string(),
            title: id.to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(1000),
            discogs_position: None,
            created_at: now,
        };

        // A prior surviving album references artist-shared, so the failed
        // import below must keep artist-shared and its image.
        db.insert_album_with_release_and_tracks(
            &album(ALBUM_PRIOR, ARTIST_SHARED),
            &release(RELEASE_PRIOR, ALBUM_PRIOR),
            &[track(TRACK_PRIOR, RELEASE_PRIOR)],
            &[],
        )
        .await
        .unwrap();

        let album_a = album(ALBUM_A, ARTIST_EXCLUSIVE);
        let release_a = release(RELEASE_A, ALBUM_A);
        let file_a = DbFile::new(
            RELEASE_A,
            "Track A.flac",
            1024,
            ContentType::Flac,
            FILE_A.to_string(),
            now,
            crate::util::fs::hash_bytes(b"fixture"),
        );
        let track_files = vec![crate::import::TrackFile::Standalone {
            db_track: track(TRACK_A, RELEASE_A),
            file_path: tmp.path().join("Track A.flac"),
        }];
        // The failed release also credits artist-shared, so both artists are
        // rollback candidates; only artist-exclusive should be deleted.
        let album_artists = vec![DbAlbumArtist {
            id: AA_SHARED.to_string(),
            album_id: ALBUM_A.to_string(),
            artist_id: ARTIST_SHARED.to_string(),
            position: 1,
            created_at: now,
        }];
        let image = |id: &str, image_type: LibraryImageType| DbLibraryImage {
            id: id.to_string(),
            blob_id: format!("{id}-blob"),
            image_type,
            content_type: ContentType::Jpeg,
            file_size: 3,
            width: None,
            height: None,
            source: "local".to_string(),
            source_url: None,
            cloud_path: None,
            content_hash: crate::util::fs::hash_bytes(&[1u8, 2, 3]),
            created_at: now,
        };
        let cover = image(RELEASE_A, LibraryImageType::Cover);
        let img_exclusive = image(ARTIST_EXCLUSIVE, LibraryImageType::Artist);
        let img_shared = image(ARTIST_SHARED, LibraryImageType::Artist);
        let bytes = [1u8, 2, 3];

        db.finalize_import_atomic(
            Some(&album_a),
            &release_a,
            &track_files,
            &[],
            &album_artists,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[file_a],
            &[],
            &[],
            Some((&cover, &bytes)),
            &[(&img_exclusive, &bytes), (&img_shared, &bytes)],
            Some((&album_a.id, &release_a.id)),
            &[],
            tmp.path().to_str().unwrap(),
            crate::config::HomeStorage::Opaque,
            &[],
        )
        .await
        .unwrap();

        // The bytes coven holds for each host-provided image, before the rollback.
        let store_dir = coven::StoreDir::new(tmp.path());
        let blob_path = |namespace: &str, blob_id: &str| {
            store_dir
                .local_blob_path(namespace, blob_id)
                .expect("a valid blob path")
        };
        // The fixture's `image` helper derives each blob id from its subject id,
        // so the stored paths are named the same way.
        let cover_blob = blob_path(crate::sync::COVERS_NAMESPACE, &format!("{RELEASE_A}-blob"));
        let exclusive_blob = blob_path(
            crate::sync::ARTIST_IMAGES_NAMESPACE,
            &format!("{ARTIST_EXCLUSIVE}-blob"),
        );
        let shared_blob = blob_path(
            crate::sync::ARTIST_IMAGES_NAMESPACE,
            &format!("{ARTIST_SHARED}-blob"),
        );
        for path in [&cover_blob, &exclusive_blob, &shared_blob] {
            assert!(path.exists(), "finalize stored {}", path.display());
        }

        db.fail_import_and_delete_release(RELEASE_A).await.unwrap();

        // The rollback declares the blobs its row deletions orphan, so coven
        // reclaims their bytes in the same write. A bare row DELETE would leave
        // these files behind forever — coven's local-blob cleanup is intent-driven
        // and only ever acts on a declared deletion.
        assert!(
            !cover_blob.exists(),
            "the failed release's cover blob is reclaimed"
        );
        assert!(
            !exclusive_blob.exists(),
            "the swept artist's image blob is reclaimed"
        );
        assert!(
            shared_blob.exists(),
            "the surviving artist still has its image blob"
        );

        // The shared artist and its image row survive; the exclusive one is gone.
        assert!(db.find_artist_by_id(ARTIST_SHARED).await.unwrap().is_some());
        assert!(db
            .find_artist_by_id(ARTIST_EXCLUSIVE)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn search_library_matches_album_artist_names() {
        let (db, _tmp) = seeded_db().await;

        let results = db.search_library("Album Artist A", 10).await.unwrap();
        assert_eq!(results.albums.len(), 1);
        assert_eq!(results.albums[0].id, ALBUM_A);
    }

    #[tokio::test]
    async fn search_library_returns_artist_hits() {
        let (db, _tmp) = seeded_db().await;

        let results = db.search_library("Album Artist A", 10).await.unwrap();
        assert_eq!(results.artists.len(), 1);
        assert_eq!(results.artists[0].artist.id, ARTIST_ALBUM);
        assert_eq!(results.artists[0].album_count, 1);

        // Composer artists are artist rows too, but only surface as artist
        // hits when they have albums; the seeded composer has none.
        let composer = db.search_library("Displayed Composer A", 10).await.unwrap();
        assert_eq!(composer.composers.len(), 1);
        assert!(composer.artists.is_empty());
    }

    #[tokio::test]
    async fn search_library_matches_composer_and_work_sort_names() {
        let (db, _tmp) = seeded_db().await;

        let composer_results = db.search_library("Hidden Composer", 10).await.unwrap();
        assert_eq!(composer_results.composers.len(), 1);
        assert_eq!(composer_results.composers[0].artist.id, ARTIST_COMPOSER);

        let work_results = db.search_library("Displayed Work", 10).await.unwrap();
        assert_eq!(work_results.works.len(), 1);
        assert_eq!(work_results.works[0].work.id, WORK_CHILD_A);
        assert_eq!(
            work_results.works[0].parent_work_id.as_deref(),
            Some(WORK_PARENT_A)
        );
        assert_eq!(
            work_results.works[0].representative_release_id.as_deref(),
            Some(RELEASE_A)
        );
    }

    #[tokio::test]
    async fn search_library_treats_like_metacharacters_as_literals() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, _updated_at, created_at)
                VALUES ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('7e9948c4-f2d0-4a73-8e5c-a885eda086ff', '50% Album Title', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, '94d062dd-9dc8-4e59-8c13-e5731b702157', 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('531c76e6-3ed8-45cd-8ee8-63366ef42031', '500 Album Title', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, '7a4de476-4ce8-4e2f-805a-d4c23dffaf8e', 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('2dd55a3f-3208-4faf-8737-453f474074cb', 'A_B Album Title', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, '07d51116-f6e3-4b4c-8ae2-601d5a972bf9', 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('e00e6a4d-b8c9-4dce-8b2a-9a4c69553abc', 'ACB Album Title', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2026, '3dc45e4f-fe7f-47aa-84fc-4d6027c57276', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('94d062dd-9dc8-4e59-8c13-e5731b702157', '7e9948c4-f2d0-4a73-8e5c-a885eda086ff', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('7a4de476-4ce8-4e2f-805a-d4c23dffaf8e', '531c76e6-3ed8-45cd-8ee8-63366ef42031', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('07d51116-f6e3-4b4c-8ae2-601d5a972bf9', '2dd55a3f-3208-4faf-8737-453f474074cb', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('3dc45e4f-fe7f-47aa-84fc-4d6027c57276', 'e00e6a4d-b8c9-4dce-8b2a-9a4c69553abc', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
                VALUES
                    ('4dc8cde9-15fb-470d-802c-b7e5f1ccc63d', '94d062dd-9dc8-4e59-8c13-e5731b702157', '50% Track Title', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('37610729-bd6c-4511-81a9-8848a832ac73', '7a4de476-4ce8-4e2f-805a-d4c23dffaf8e', '500 Track Title', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('b2930937-dae6-4719-8150-aa61422eeeac', '07d51116-f6e3-4b4c-8ae2-601d5a972bf9', 'A_B Track Title', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('1f084b57-7c9c-457f-80fb-c61688c31175', '3dc45e4f-fe7f-47aa-84fc-4d6027c57276', 'ACB Track Title', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let percent_results = db.search_library("50%", 10).await.unwrap();
        assert_eq!(percent_results.albums.len(), 1);
        assert_eq!(percent_results.albums[0].id, ALBUM_PERCENT);
        assert_eq!(percent_results.tracks.len(), 1);
        assert_eq!(percent_results.tracks[0].id, TRACK_PERCENT);

        let underscore_results = db.search_library("A_B", 10).await.unwrap();
        assert_eq!(underscore_results.albums.len(), 1);
        assert_eq!(underscore_results.albums[0].id, ALBUM_UNDERSCORE);
        assert_eq!(underscore_results.tracks.len(), 1);
        assert_eq!(underscore_results.tracks[0].id, TRACK_UNDERSCORE);
    }

    #[tokio::test]
    async fn composer_detail_carries_work_parent_and_representative_release() {
        let (db, _tmp) = seeded_db().await;

        let detail = db
            .find_composer_detail(ARTIST_COMPOSER)
            .await
            .unwrap()
            .expect("composer detail");

        assert_eq!(detail.work_groups.len(), 1);
        let group = &detail.work_groups[0];
        assert_eq!(
            group.parent.as_ref().map(|work| work.work.id.as_str()),
            Some(WORK_PARENT_A)
        );
        assert_eq!(group.works.len(), 1);
        assert_eq!(group.works[0].work.id, WORK_CHILD_A);
        assert_eq!(
            group.works[0].parent_work_id.as_deref(),
            Some(WORK_PARENT_A)
        );
        assert_eq!(
            group.works[0].representative_release_id.as_deref(),
            Some(RELEASE_A)
        );
    }

    #[tokio::test]
    async fn work_detail_lists_child_works_with_their_representative_release() {
        let (db, _tmp) = seeded_db().await;

        let detail = db
            .find_work_detail(WORK_PARENT_A)
            .await
            .unwrap()
            .expect("work detail");

        assert_eq!(detail.child_works.len(), 1);
        assert_eq!(detail.child_works[0].work.id, WORK_CHILD_A);
        assert_eq!(
            detail.child_works[0].representative_release_id.as_deref(),
            Some(RELEASE_A)
        );
    }

    #[tokio::test]
    async fn work_detail_release_rows_carry_album_release_display_fields() {
        let (db, _tmp) = seeded_db().await;

        let detail = db
            .find_work_detail(WORK_CHILD_A)
            .await
            .unwrap()
            .expect("work detail");

        assert_eq!(detail.releases.len(), 1);
        let release = &detail.releases[0];
        assert_eq!(release.release_id, RELEASE_A);
        assert_eq!(release.album_id, ALBUM_A);
        assert_eq!(release.album_title, "Album Title A");
        assert_eq!(release.release_name, None);
        assert_eq!(release.year, Some(2026));
        assert_eq!(release.format.as_deref(), Some("CD"));
        assert_eq!(release.release_index, 1);
    }

    #[tokio::test]
    async fn composer_page_uses_id_tiebreaker() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('80cd3a5e-7fb7-4766-8ec3-d8e86575743b', 'Composer Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('5dcc4999-03bd-42cc-8d14-8bf0a05effa3', 'Composer Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('2b748d47-e5b7-4c40-8716-1e608b9dfc3d', 'Composer Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO works (id, title, work_type, _updated_at, created_at)
                VALUES
                    ('432c8996-8af0-43dc-868a-822a256f65c4', 'Work Title A', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('d866f97d-e57f-45e8-8c4e-f81ad8717882', 'Work Title B', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('00e1ff99-c327-477d-846d-28d2f27fa004', 'Work Title C', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
                VALUES
                    ('f6fc3bd0-99db-4e6e-8efa-c8e5a1c35d74', '00e1ff99-c327-477d-846d-28d2f27fa004', '80cd3a5e-7fb7-4766-8ec3-d8e86575743b', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('ec41a8cd-a9a4-473e-8b70-d78168aefd8e', '432c8996-8af0-43dc-868a-822a256f65c4', '5dcc4999-03bd-42cc-8d14-8bf0a05effa3', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('08d921ba-8e6b-4b5b-8583-498117bbefe4', 'd866f97d-e57f-45e8-8c4e-f81ad8717882', '2b748d47-e5b7-4c40-8716-1e608b9dfc3d', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let sort = [ComposerSortCriterion {
            field: ComposerSortField::WorkCount,
            direction: SortDirection::Descending,
        }];
        let mut page_ids = Vec::new();
        for offset in 0..3 {
            let page = db.get_composer_page(&sort, offset, 1).await.unwrap();
            assert_eq!(page.len(), 1);
            page_ids.push(page[0].artist.id.clone());
        }

        // Equal work counts, so the id breaks the tie ascending — see the
        // artist-page tiebreaker test for why the expectation is sorted.
        let mut expected = vec![COMPOSER_A, COMPOSER_B, COMPOSER_C];
        expected.sort();
        assert_eq!(page_ids, expected);
    }

    /// A secondary criterion applies before the name-ASC tail. Two composers tie on
    /// `WorkCount` but differ in name, so `[WorkCount DESC, Name DESC]` must order
    /// the tied pair by name descending — a single-criterion implementation would
    /// fall through to the tail's `composer.name ASC` and order them the other way.
    #[tokio::test]
    async fn composer_page_applies_secondary_criterion() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('5dcc4999-03bd-42cc-8d14-8bf0a05effa3', 'Composer Name A', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('2b748d47-e5b7-4c40-8716-1e608b9dfc3d', 'Composer Name B', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('4d93d615-4549-45d9-81d9-644f079d59bf', 'Composer Name Solo', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO works (id, title, work_type, _updated_at, created_at)
                VALUES
                    ('1d446150-576e-479f-87a7-40ac7a511fa1', 'Work Title A1', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('6a32dca0-bf5b-4baa-829d-dc2ef531e763', 'Work Title A2', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('735e697c-f2ce-4512-806c-4f872446f6e6', 'Work Title B1', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('d5f30cc0-a35a-4294-851b-ce2d9c172d1c', 'Work Title B2', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('5dc2446b-5241-46e0-8be4-4325e06f1417', 'Work Title Solo', 'work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
                VALUES
                    ('c6ad84f8-3ff6-4528-8b5e-f90d24c33908', '1d446150-576e-479f-87a7-40ac7a511fa1', '5dcc4999-03bd-42cc-8d14-8bf0a05effa3', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('b6b51719-ad5f-4e9e-8aae-0b51883677ce', '6a32dca0-bf5b-4baa-829d-dc2ef531e763', '5dcc4999-03bd-42cc-8d14-8bf0a05effa3', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('72f0c48b-5fcf-40ff-8fff-3dc15a0cf06d', '735e697c-f2ce-4512-806c-4f872446f6e6', '2b748d47-e5b7-4c40-8716-1e608b9dfc3d', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('7b13d5f1-3836-422f-831a-10ac1435a502', 'd5f30cc0-a35a-4294-851b-ce2d9c172d1c', '2b748d47-e5b7-4c40-8716-1e608b9dfc3d', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('aeccde70-29b8-4f81-85c4-5135f2c7a40c', '5dc2446b-5241-46e0-8be4-4325e06f1417', '4d93d615-4549-45d9-81d9-644f079d59bf', 0, 'file_tags', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let sort = [
            ComposerSortCriterion {
                field: ComposerSortField::WorkCount,
                direction: SortDirection::Descending,
            },
            ComposerSortCriterion {
                field: ComposerSortField::Name,
                direction: SortDirection::Descending,
            },
        ];
        let page = db.get_composer_page(&sort, 0, 10).await.unwrap();
        let ids: Vec<&str> = page.iter().map(|c| c.artist.id.as_str()).collect();

        // composer-a and composer-b tie on work_count (2 each); the secondary
        // Name DESC criterion orders composer-b before composer-a.
        assert_eq!(ids, vec![COMPOSER_B, COMPOSER_A, COMPOSER_SOLO]);
    }
}

#[cfg(test)]
mod artist_mode_tests {
    use super::super::*;
    use super::*;
    use coven::SystemClock;

    /// Artists covering every membership case: a primary-FK artist that is
    /// also a junction artist elsewhere, a junction-only artist, the Various
    /// Artists row as a compilation's primary, a work-only composer (no album
    /// links), and a fully unlinked artist.
    async fn seeded_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name B', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('7fa00099-f5d8-4ec2-88bd-e19d8edd7bb8', 'Artist Name A', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('f862abf2-3b15-4518-889b-1996d7100201', 'Various Artists', NULL, '194', NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('b96d8066-777d-408d-8ae4-ed58c767e40c', 'Composer Name A', NULL, NULL, 'mb-artist-work-only', 'stamp', '2026-01-01T00:00:00Z'),
                    ('7d8362d9-b321-495a-89f7-4cd8998449a4', 'Artist Name C', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2001, NULL, 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('a0231b0b-549b-4e4d-806f-a4b66373e087', 'Album Title B', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 1999, NULL, 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('20022731-6ca1-4bf3-8d27-1e2b2e5e9816', 'Compilation Title A', 'f862abf2-3b15-4518-889b-1996d7100201', 2005, NULL, 1, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('64e79a1f-404a-4c34-809a-a3cb44bf1942', 'a0231b0b-549b-4e4d-806f-a4b66373e087', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('77f4af5a-9661-4fb7-845b-73901b0a3ebd', '20022731-6ca1-4bf3-8d27-1e2b2e5e9816', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');

                -- artist-extra joins album-b; artist-primary's junction row on
                -- album-a duplicates its primary FK and must not double-count.
                INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
                VALUES
                    ('04ebd233-5eef-4f8c-8ca1-6612601fd136', 'a0231b0b-549b-4e4d-806f-a4b66373e087', '7fa00099-f5d8-4ec2-88bd-e19d8edd7bb8', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('34392f8d-93a5-47ad-8fe1-f8f5ce013123', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 1, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO works (id, title, disambiguation, work_type, _updated_at, created_at)
                VALUES ('432c8996-8af0-43dc-868a-822a256f65c4', 'Work Title A', NULL, 'work', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
                VALUES ('ec41a8cd-a9a4-473e-8b70-d78168aefd8e', '432c8996-8af0-43dc-868a-822a256f65c4', 'b96d8066-777d-408d-8ae4-ed58c767e40c', 0, 'musicbrainz', 'stamp', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();
        (db, tmp)
    }

    #[tokio::test]
    async fn artist_page_lists_album_artists_with_distinct_album_counts() {
        let (db, _tmp) = seeded_db().await;

        let sort = [ArtistSortCriterion {
            field: ArtistSortField::Name,
            direction: SortDirection::Ascending,
        }];
        let page = db.get_artist_page(&sort, 0, 10).await.unwrap();

        let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();
        assert_eq!(ids, vec![ARTIST_EXTRA, ARTIST_PRIMARY, ARTIST_VARIOUS]);

        let counts: Vec<i64> = page.iter().map(|a| a.album_count).collect();
        assert_eq!(counts, vec![1, 2, 1]);

        assert_eq!(db.get_artist_count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn artist_page_sorts_by_album_count() {
        let (db, _tmp) = seeded_db().await;

        let sort = [ArtistSortCriterion {
            field: ArtistSortField::AlbumCount,
            direction: SortDirection::Descending,
        }];
        let page = db.get_artist_page(&sort, 0, 10).await.unwrap();

        let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();
        assert_eq!(ids, vec![ARTIST_PRIMARY, ARTIST_EXTRA, ARTIST_VARIOUS]);
    }

    #[tokio::test]
    async fn artist_page_uses_id_tiebreaker() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('1b4bafc9-0ece-4538-833e-4ff52feb6ef0', 'Artist Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('d7d8141f-54ff-467d-8b60-4f34a4d2e528', 'Artist Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('38fc314c-c130-4120-8ca9-38b870ccef3a', 'Artist Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('881dc5cf-0686-456a-87c9-98c50e775177', 'Album Title C', '1b4bafc9-0ece-4538-833e-4ff52feb6ef0', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('a0231b0b-549b-4e4d-806f-a4b66373e087', 'Album Title B', '38fc314c-c130-4120-8ca9-38b870ccef3a', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('599bc437-136f-4643-87a6-ac30c3fae614', '881dc5cf-0686-456a-87c9-98c50e775177', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('64e79a1f-404a-4c34-809a-a3cb44bf1942', 'a0231b0b-549b-4e4d-806f-a4b66373e087', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let sort = [ArtistSortCriterion {
            field: ArtistSortField::AlbumCount,
            direction: SortDirection::Descending,
        }];
        let mut page_ids = Vec::new();
        for offset in 0..3 {
            let page = db.get_artist_page(&sort, offset, 1).await.unwrap();
            assert_eq!(page.len(), 1);
            page_ids.push(page[0].artist.id.clone());
        }

        // Equal album counts, so the id breaks the tie ascending. The ids are
        // UUIDs, so the expectation is those same three sorted — asserting the
        // rule rather than a hand-written order that depends on which UUID the
        // fixture happened to mint.
        let mut expected = vec![ARTIST_A, ARTIST_B, ARTIST_C];
        expected.sort();
        assert_eq!(page_ids, expected);
    }

    /// A secondary criterion applies before the name-ASC tail. Two artists tie on
    /// `AlbumCount` but differ in name, so `[AlbumCount DESC, Name DESC]` must order
    /// the tied pair by name descending — a single-criterion implementation would
    /// fall through to the tail's `ar.name ASC` and order them the other way.
    #[tokio::test]
    async fn artist_page_applies_secondary_criterion() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('d7d8141f-54ff-467d-8b60-4f34a4d2e528', 'Artist Name A', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('38fc314c-c130-4120-8ca9-38b870ccef3a', 'Artist Name B', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('49549823-0e72-4747-891e-ee50e1611e3a', 'Artist Name Solo', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('1ac5b125-782f-4dab-8669-417e804d02bb', 'Album Title A1', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('f270285f-ef24-4daa-8058-0dd91571843e', 'Album Title A2', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('2d705181-42c1-47bd-822c-245d8db41d60', 'Album Title B1', '38fc314c-c130-4120-8ca9-38b870ccef3a', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('191beebd-146b-431f-8a87-ed4be0db20b7', 'Album Title B2', '38fc314c-c130-4120-8ca9-38b870ccef3a', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('831374ed-abd3-4b6d-84b1-15a9974ecadc', 'Album Title Solo', '49549823-0e72-4747-891e-ee50e1611e3a', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('5611ca91-e045-490a-8c14-89c3181a92ab', '1ac5b125-782f-4dab-8669-417e804d02bb', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('48c59983-901e-455e-8cb0-ac0011c08bb4', 'f270285f-ef24-4daa-8058-0dd91571843e', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('7680c516-307b-4a2a-8aea-850f976e006e', '2d705181-42c1-47bd-822c-245d8db41d60', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('51ca3994-c53d-45e4-8ad2-04ac3f4181c8', '191beebd-146b-431f-8a87-ed4be0db20b7', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('7ae8e6fc-98bd-46e2-82c0-4b913087deb1', '831374ed-abd3-4b6d-84b1-15a9974ecadc', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let sort = [
            ArtistSortCriterion {
                field: ArtistSortField::AlbumCount,
                direction: SortDirection::Descending,
            },
            ArtistSortCriterion {
                field: ArtistSortField::Name,
                direction: SortDirection::Descending,
            },
        ];
        let page = db.get_artist_page(&sort, 0, 10).await.unwrap();
        let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();

        // artist-a and artist-b tie on album_count (2 each); the secondary
        // Name DESC criterion orders artist-b before artist-a.
        assert_eq!(ids, vec![ARTIST_B, ARTIST_A, ARTIST_SOLO]);
    }

    #[tokio::test]
    async fn artist_detail_orders_albums_year_then_title_with_unknown_years_last() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        db.call(|conn| {
            conn.execute_batch(
                "
                INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
                VALUES
                    ('d7d8141f-54ff-467d-8b60-4f34a4d2e528', 'Artist Name A', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('4d0b27b7-c953-47f5-8614-70ed973923dc', 'Artist Name B', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('c6648d5a-617e-4b69-87da-b7f1c4fb5e65', 'Album Title Null', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', NULL, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('a663cff7-fad7-45b1-8469-5f77af82ddb8', 'Album Title B', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2001, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('88183677-683b-485e-8224-f6a328c233c7', 'album title a', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2001, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('88f57246-3e65-4eb9-8d36-ee8d40326cfc', 'Album Title 1999', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 1999, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('7e6f42e7-8952-48e6-89bf-d1bcc611176d', 'Album Title Junction', '4d0b27b7-c953-47f5-8614-70ed973923dc', 2005, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('ebef769a-f2a3-4443-8c01-50921de47fbb', 'Album Title Unrelated', '4d0b27b7-c953-47f5-8614-70ed973923dc', 1990, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('f09739aa-4dde-4cbc-8daf-d77eb2f980ff', 'c6648d5a-617e-4b69-87da-b7f1c4fb5e65', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('f582ee48-93a9-4152-8a12-7a9b62f86c2a', 'a663cff7-fad7-45b1-8469-5f77af82ddb8', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('7361d9e4-6e06-4a57-84fa-c6042cb2fb78', '88183677-683b-485e-8224-f6a328c233c7', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('0014a775-79c9-419f-849e-8de944a1ef04', '88f57246-3e65-4eb9-8d36-ee8d40326cfc', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('d981c4b7-e727-41f9-826e-31ec67066d8c', '7e6f42e7-8952-48e6-89bf-d1bcc611176d', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('4fad85ec-94ef-4de6-8cf8-79dc25389051', 'ebef769a-f2a3-4443-8c01-50921de47fbb', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

                INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
                VALUES ('1725fb34-3b73-477f-8113-e995104feae3', '7e6f42e7-8952-48e6-89bf-d1bcc611176d', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        let detail = db.find_artist_detail(ARTIST_A).await.unwrap();
        let detail = detail.expect("artist-a has album links and must resolve");

        assert_eq!(detail.artist.artist.id, ARTIST_A);
        assert_eq!(detail.artist.album_count, 5);

        let album_ids: Vec<&str> = detail.albums.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            album_ids,
            vec![
                ALBUM_1999,
                ALBUM_2001_LOWER,
                ALBUM_2001_UPPER,
                ALBUM_JUNCTION,
                ALBUM_NULL,
            ]
        );
    }

    #[tokio::test]
    async fn artist_detail_absent_or_album_less_artist_is_none() {
        let (db, _tmp) = seeded_db().await;

        assert!(db
            .find_artist_detail(ARTIST_ABSENT)
            .await
            .unwrap()
            .is_none());
        assert!(db
            .find_artist_detail(ARTIST_WORK_ONLY)
            .await
            .unwrap()
            .is_none());
    }
}

#[cfg(test)]
mod playback_state_load_tests {
    use super::super::*;
    use coven::SystemClock;

    async fn empty_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        (db, tmp)
    }

    /// `source` and `shuffled` are written together, so a row carrying one
    /// without the other is corrupt: `load_playback_state` reports `Corrupt`
    /// rather than inventing a flag or masking it as an absent cache.
    #[tokio::test]
    async fn mismatched_source_and_shuffled_discards_the_cache() {
        let (db, _tmp) = empty_db().await;

        // Write a row by hand with a present source but a NULL shuffled --
        // `save_playback_state` never produces this, so we insert it directly.
        db.call(|conn| {
            conn.execute(
                "INSERT INTO playback_state \
                     (id, source, shuffled, manual, repeat, \
                      current_track_id, position_ms, volume, is_muted) \
                     VALUES ('current', 'cccb6034-5922-40d2-8d0b-d94619230882', NULL, '[]', 'off', \
                      NULL, NULL, 1.0, 0)",
                [],
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();

        assert!(matches!(
            db.load_playback_state().await.unwrap(),
            LoadedPlaybackState::Corrupt
        ));
    }
}

/// `import_candidate_state` — device-local derived state keyed by
/// `CategorizedFiles::content_hash`: what identification concluded, and what the
/// user decided about the folder's track sheets. The mechanism's own round-trip
/// proves little on its own; these also nail down the two things that make the
/// hash the row's whole identity (resizing invalidates, moving doesn't), the
/// asymmetry the design leans on (a terminal verdict is stored, a transport
/// failure is not), and the pair that makes a re-bound candidate re-identify
/// rather than trust a verdict about a shape it no longer has.
#[cfg(test)]
mod import_candidate_state_tests {
    use super::super::*;
    use crate::identify::{GroupKey, ResultProvenance, TerminalVerdict};
    use crate::import::folder_registry::host_root;
    use crate::import::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScannedFile};
    use crate::import::search::MetadataResult;
    use coven::FixedClock;
    use std::path::PathBuf;

    /// The instant `empty_db`'s injected clock always returns. Fixed rather
    /// than `SystemClock` so `identified_at` can be asserted exactly — which is
    /// why `save_import_candidate_verdict` stamps it from the injected clock
    /// instead of taking it from the caller.
    fn fixed_identified_at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    async fn empty_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(FixedClock(fixed_identified_at())),
            Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        (db, tmp)
    }

    /// A folder of plain track files (no track sheet) named
    /// `(relative_path, size)`.
    fn track_files_candidate(files: &[(&str, u64)]) -> CategorizedFiles {
        CategorizedFiles {
            files: files
                .iter()
                .map(|(name, size)| CandidateFile {
                    file: ScannedFile::new(PathBuf::from(*name), name.to_string(), *size),
                    role: FileRole::Audio,
                    proposed_audio: true,
                })
                .collect(),
            format_label: "FLAC".to_string(),
        }
    }

    fn sample_verdict() -> TerminalVerdict {
        TerminalVerdict::Found {
            matches: vec![MetadataResult {
                source: MetadataSource::MusicBrainz,
                release_id: "rel-1".to_string(),
                title: "Album".to_string(),
                artist: Some("Artist".to_string()),
                year: Some(1999),
                format: Some("CD".to_string()),
                label: Some("Label".to_string()),
                catalog_number: Some("CAT-1".to_string()),
                country: Some("US".to_string()),
                cover_art: None,
                source_group_id: Some("group-1".to_string()),
                source_tracks: None,
            }],
            track_count: 11,
            group: GroupKey {
                source: MetadataSource::MusicBrainz,
                source_group_id: "group-1".to_string(),
            },
            provenance: vec![ResultProvenance {
                by_disc_id: true,
                by_barcode: false,
                matches_catalog: true,
            }],
        }
    }

    fn new_candidate_row(
        content_hash: &str,
        folder_path: &str,
        verdict: &TerminalVerdict,
        probed_total_duration_ms: i64,
    ) -> NewImportCandidateVerdict {
        NewImportCandidateVerdict {
            content_hash: content_hash.to_string(),
            folder_path: folder_path.to_string(),
            verdict: serde_json::to_string(verdict).unwrap(),
            probed_total_duration_ms,
            expected_edit_revision: 0,
        }
    }

    /// Save a verdict, read it back, and check the provenance survived the JSON
    /// round trip along with everything else — a stripped `by_disc_id` or a
    /// dropped catalog number wouldn't show up in a looser comparison.
    #[tokio::test]
    async fn round_trip_preserves_the_verdict_including_provenance() {
        let (db, _tmp) = empty_db().await;
        let candidate =
            track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
        let hash = candidate.content_hash();
        let verdict = sample_verdict();
        let row = new_candidate_row(&hash, "/music/Some Album", &verdict, 2_700_000);

        db.save_import_candidate_verdict(&row).await.unwrap();

        let loaded = db.load_import_candidate_states().await.unwrap();
        let loaded_row = loaded
            .get(&hash)
            .expect("row present under its content hash");
        assert_eq!(loaded_row.folder_path, "/music/Some Album");
        let identify = loaded_row
            .identify
            .as_ref()
            .expect("a stored verdict reads back as an identify result");
        assert_eq!(identify.probed_total_duration_ms, 2_700_000);
        // Stamped by the write path from the injected clock, not something
        // `new_candidate_row` had any way to supply.
        assert_eq!(identify.identified_at, fixed_identified_at());
        let loaded_verdict: TerminalVerdict = serde_json::from_str(&identify.verdict).unwrap();
        assert_eq!(
            loaded_verdict, verdict,
            "the verdict must round-trip exactly, provenance included"
        );
    }

    /// Resizing one file changes `content_hash`, which is the whole
    /// invalidation mechanism: the new hash finds nothing (so it gets
    /// re-identified) while the old row is left behind, unreachable, under its
    /// own key.
    #[tokio::test]
    async fn resizing_a_file_orphans_the_old_row_under_a_new_hash() {
        let (db, _tmp) = empty_db().await;
        let original =
            track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
        let original_hash = original.content_hash();
        let row = new_candidate_row(
            &original_hash,
            "/music/Some Album",
            &sample_verdict(),
            2_700_000,
        );
        db.save_import_candidate_verdict(&row).await.unwrap();

        let resized =
            track_files_candidate(&[("01 Track.flac", 999_999), ("02 Track.flac", 234_567)]);
        let resized_hash = resized.content_hash();
        assert_ne!(
            original_hash, resized_hash,
            "resizing a file must change the hash"
        );

        let loaded = db.load_import_candidate_states().await.unwrap();
        assert!(
            loaded.contains_key(&original_hash),
            "the old row is still present under its own key"
        );
        assert!(
            !loaded.contains_key(&resized_hash),
            "the new hash must find no row -- the candidate needs re-identifying"
        );
    }

    /// Same files, same relative paths and sizes, under a different parent
    /// directory: `content_hash` never looks at the absolute path, so the row
    /// saved for the folder at its old location is still the row found for it
    /// at the new one.
    #[tokio::test]
    async fn a_moved_folder_hashes_identically_and_keeps_its_row() {
        let (db, _tmp) = empty_db().await;
        let at_old_location =
            track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
        let hash = at_old_location.content_hash();
        let row = new_candidate_row(
            &hash,
            "/music/Old Location/Some Album",
            &sample_verdict(),
            2_700_000,
        );
        db.save_import_candidate_verdict(&row).await.unwrap();

        let at_new_location = CategorizedFiles {
            files: vec![
                CandidateFile {
                    file: ScannedFile::new(
                        PathBuf::from("/music/New Location/Some Album/01 Track.flac"),
                        "01 Track.flac".to_string(),
                        123_456,
                    ),
                    role: FileRole::Audio,
                    proposed_audio: true,
                },
                CandidateFile {
                    file: ScannedFile::new(
                        PathBuf::from("/music/New Location/Some Album/02 Track.flac"),
                        "02 Track.flac".to_string(),
                        234_567,
                    ),
                    role: FileRole::Audio,
                    proposed_audio: true,
                },
            ],
            format_label: "FLAC".to_string(),
        };
        assert_eq!(
            hash,
            at_new_location.content_hash(),
            "a moved folder must hash identically to itself before the move"
        );

        let loaded = db.load_import_candidate_states().await.unwrap();
        assert!(
            loaded.contains_key(&at_new_location.content_hash()),
            "the row saved before the move must still be reachable after it"
        );
    }

    /// A transport failure writes no row. Driven through the real identify
    /// reducer (a disc-ID lookup that fails over the network, no barcode
    /// source to fall back on) rather than hand-built, so this actually
    /// exercises the guard in `identify::verdict::TerminalVerdict::try_from` —
    /// the one thing standing between "nothing was learned" and a permanent
    /// `NotFoundAnywhere` row. If that guard is ever weakened or removed, this
    /// starts writing a row and fails.
    #[tokio::test]
    async fn no_row_is_written_for_a_transport_failure() {
        use crate::identify::state::step as identify_step;
        use crate::identify::{IdentifyEvent, IdentifyState};
        use crate::signals::{BarcodeSignal, DiscIdSignal, LookupFailure, Signals, TextSignal};

        let (db, _tmp) = empty_db().await;
        let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
        let hash = candidate.content_hash();

        let (state, _) = identify_step(IdentifyState::Idle, IdentifyEvent::Started);
        let (state, _) = identify_step(
            state,
            IdentifyEvent::SignalsUpdated {
                signals: Signals {
                    disc_id: DiscIdSignal::Computed {
                        disc_id: "disc-hash".to_string(),
                        track_count: 1,
                    },
                    barcode: BarcodeSignal::Absent,
                    text: TextSignal::Settled {
                        catalogs: vec![],
                        free_text: vec![],
                    },
                    probed_total_duration_ms: 0,
                },
            },
        );
        let (state, _) = identify_step(
            state,
            IdentifyEvent::DiscidLookupFailed {
                failure: LookupFailure::Provider { status: Some(503) },
                track_count: 1,
            },
        );

        // Exactly the shape a scheduler will use: only a successful conversion
        // ever reaches `save_import_candidate_verdict`.
        if let Ok(verdict) = TerminalVerdict::try_from(state) {
            let row = new_candidate_row(&hash, "/music/Some Album", &verdict, 0);
            db.save_import_candidate_verdict(&row).await.unwrap();
        }

        let loaded = db.load_import_candidate_states().await.unwrap();
        assert!(
            !loaded.contains_key(&hash),
            "a transport failure teaches nothing -- absence is the retry signal"
        );
    }

    /// A binding the user set survives a relaunch: it is stored under the
    /// candidate's content hash, read back from a cold database, and the scan
    /// that follows reports the folder as they settled it rather than as its
    /// filenames read.
    ///
    /// The scan is the point — a binding that round-tripped through SQLite but
    /// never reached a folder's roles would be a stored value nothing consumes.
    #[tokio::test]
    async fn a_binding_survives_a_relaunch() {
        use crate::import::folder_scanner::{
            collect_release_candidate_files_with_scope, CandidateFileEdits, SheetBindingEdits,
            StoredCandidateEdits, UserSheetBinding,
        };

        let (db, _tmp) = empty_db().await;
        let folder = walkthrough_folder();
        let scanned = collect_release_candidate_files_with_scope(
            folder.path(),
            crate::import::ReleaseFileScope::Recursive,
            &StoredCandidateEdits::none(),
        )
        .unwrap();
        assert_eq!(scanned.track_count(), 1, "unbound, the image is one track");
        let root = folder.path().to_string_lossy().into_owned();
        db.add_watched_import_folder(&root).await.unwrap();
        let generation = db.begin_folder_scan(&root).await.unwrap();
        let candidate = crate::import::folder_scanner::FolderCandidate {
            path: folder.path().to_path_buf(),
            file_root: folder.path().to_path_buf(),
            name: "Release".to_string(),
            files: scanned.clone(),
            watched_folder_path: root.clone(),
            scope: crate::import::ReleaseFileScope::Recursive,
            file_edit_revision: 0,
            display_path: String::new(),
            resolved_boundaries: Vec::new(),
            combine_ancestor_key: None,
        };
        db.save_folder_scan_item(
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(candidate),
            &[],
        )
        .await
        .unwrap();
        db.finish_folder_scan(&root, generation, None)
            .await
            .unwrap();

        let mut edits = SheetBindingEdits::default();
        edits.set(
            "cd.cue".to_string(),
            UserSheetBinding::Describes {
                file_id: "cd.flac".to_string(),
            },
        );
        let candidate_edits = CandidateFileEdits {
            sheet_bindings: edits,
            ..Default::default()
        };
        let mut settled = scanned.clone();
        settled
            .apply_candidate_file_edits(&candidate_edits)
            .unwrap();
        db.save_import_candidate_file_edits(
            &scanned.content_hash(),
            &folder.path().to_string_lossy(),
            0,
            &candidate_edits,
            &[(folder.path().to_string_lossy().into_owned(), settled)],
        )
        .await
        .unwrap();

        let current = db
            .load_candidate_file_edits(&scanned.content_hash())
            .await
            .unwrap();
        assert_eq!(current.revision, 1);
        assert_eq!(
            current.sheet_bindings.get("cd.cue"),
            Some(&UserSheetBinding::Describes {
                file_id: "cd.flac".to_string()
            })
        );
        assert_eq!(
            db.load_candidate_file_edits("missing").await.unwrap(),
            CandidateFileEdits::default()
        );

        let restored = db.load_folder_scan_snapshots().await.unwrap();
        let crate::import::folder_scanner::ScanItem::Valid(restored_candidate) =
            &restored[0].items[0]
        else {
            panic!("the persisted candidate keeps its valid variant");
        };
        assert_eq!(restored_candidate.file_edit_revision, 1);
        assert_eq!(restored_candidate.track_count(), 12);
        assert_eq!(
            restored_candidate.files.bound_sheets()[0].audio.file_name,
            "cd.flac"
        );

        // A subsequent scan reads the same decisions and derives the same
        // shape as the candidate restored before that scan.
        let stored = db.load_stored_candidate_edits().await.unwrap();
        let reopened = collect_release_candidate_files_with_scope(
            folder.path(),
            crate::import::ReleaseFileScope::Recursive,
            &stored,
        )
        .unwrap();

        assert_eq!(
            reopened.track_count(),
            12,
            "the binding read back from disk is the one the scan applies"
        );
        assert_eq!(reopened.bound_sheets()[0].audio.file_name, "cd.flac");
    }

    /// The pair that makes re-identification correct rather than incidental:
    /// changing a binding leaves the row's key alone, **and** clears the
    /// verdict stored under it.
    ///
    /// The hash covers files and never role decisions, so the edit addresses
    /// the same row rather than orphaning it — and that row's verdict was
    /// derived from the shape the folder no longer has, so the queue must
    /// answer the candidate again instead of trusting it.
    #[tokio::test]
    async fn changing_a_binding_keeps_the_hash_and_clears_the_verdict() {
        use crate::import::folder_scanner::{
            collect_release_candidate_files_with_scope, CandidateFileEdits, SheetBindingEdits,
            StoredCandidateEdits, UserSheetBinding,
        };

        let (db, _tmp) = empty_db().await;
        let folder = walkthrough_folder();
        let unbound = collect_release_candidate_files_with_scope(
            folder.path(),
            crate::import::ReleaseFileScope::Recursive,
            &StoredCandidateEdits::none(),
        )
        .unwrap();
        let hash = unbound.content_hash();

        db.save_import_candidate_verdict(&new_candidate_row(
            &hash,
            &folder.path().to_string_lossy(),
            &sample_verdict(),
            2_700_000,
        ))
        .await
        .unwrap();
        assert!(
            db.load_import_candidate_states()
                .await
                .unwrap()
                .get(&hash)
                .expect("the verdict is stored")
                .identify
                .is_some(),
            "the candidate starts out identified"
        );

        let mut edits = SheetBindingEdits::default();
        edits.set(
            "cd.cue".to_string(),
            UserSheetBinding::Describes {
                file_id: "cd.flac".to_string(),
            },
        );
        db.save_import_candidate_file_edits(
            &hash,
            &folder.path().to_string_lossy(),
            0,
            &CandidateFileEdits {
                sheet_bindings: edits,
                ..Default::default()
            },
            &[],
        )
        .await
        .unwrap();

        let bound = collect_release_candidate_files_with_scope(
            folder.path(),
            crate::import::ReleaseFileScope::Recursive,
            &db.load_stored_candidate_edits().await.unwrap(),
        )
        .unwrap();
        assert_eq!(
            bound.track_count(),
            12,
            "the folder really did change shape -- otherwise this proves nothing"
        );
        assert_eq!(
            bound.content_hash(),
            hash,
            "the hash covers files, never role decisions, so the row stays addressable"
        );

        let row = db
            .load_import_candidate_states()
            .await
            .unwrap()
            .remove(&hash)
            .expect("the row is still found under the unchanged hash");
        assert!(
            row.identify.is_none(),
            "the stored verdict described the folder before the binding; it must be cleared \
             so the queue identifies the candidate again"
        );
        assert_eq!(
            row.file_edits.sheet_bindings.get("cd.cue"),
            Some(&UserSheetBinding::Describes {
                file_id: "cd.flac".to_string()
            }),
            "the decision that cleared the verdict is what the row now holds"
        );
    }

    #[tokio::test]
    async fn folder_release_decision_is_idempotent_and_root_scoped() {
        use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

        let (db, _tmp) = empty_db().await;
        let other = host_root("/other/library");
        let key = FolderReleaseDecisionKey {
            watched_folder_path: host_root("/mounted/library"),
            relative_folder_path: "Collection/Release Wrapper".to_string(),
        };
        db.add_watched_import_folder(&key.watched_folder_path)
            .await
            .unwrap();
        db.add_watched_import_folder(&other).await.unwrap();

        db.set_folder_release_decision(&key, FolderReleaseDecision::CombineAsOneRelease)
            .await
            .unwrap();
        db.set_folder_release_decision(&key, FolderReleaseDecision::CombineAsOneRelease)
            .await
            .unwrap();
        db.set_folder_release_decision(
            &FolderReleaseDecisionKey {
                watched_folder_path: other,
                relative_folder_path: key.relative_folder_path.clone(),
            },
            FolderReleaseDecision::KeepAsSeparateReleases,
        )
        .await
        .unwrap();

        let decisions = db
            .load_folder_release_decisions(&key.watched_folder_path)
            .await
            .unwrap();
        assert_eq!(
            decisions.get(&key.relative_folder_path),
            Some(FolderReleaseDecision::CombineAsOneRelease)
        );
    }

    fn scanned_candidate(root: &str, name: &str) -> crate::import::folder_scanner::ScanItem {
        use crate::import::folder_scanner::{FolderCandidate, ReleaseFileScope, ScanItem};

        let path = PathBuf::from(root).join(name);
        ScanItem::Valid(FolderCandidate {
            path: path.clone(),
            file_root: path,
            name: name.to_string(),
            files: track_files_candidate(&[("01.flac", 123)]),
            watched_folder_path: root.to_string(),
            scope: ReleaseFileScope::Direct,
            file_edit_revision: 0,
            display_path: name.to_string(),
            resolved_boundaries: Vec::new(),
            combine_ancestor_key: None,
        })
    }

    #[tokio::test]
    async fn folder_scan_cache_writes_progressively_and_prunes_only_on_success() {
        let (db, _tmp) = empty_db().await;
        let root = &host_root("/mounted/library");
        let first = scanned_candidate(root, "First");
        let second = scanned_candidate(root, "Second");
        db.add_watched_import_folder(root).await.unwrap();

        let generation = db.begin_folder_scan(root).await.unwrap();
        assert!(db
            .save_folder_scan_item(root, generation, &first, &[])
            .await
            .unwrap());
        assert!(db
            .finish_folder_scan(root, generation, Some("share disconnected"))
            .await
            .unwrap());

        let generation = db.begin_folder_scan(root).await.unwrap();
        assert!(db
            .save_folder_scan_item(root, generation, &second, &[])
            .await
            .unwrap());
        assert!(db
            .finish_folder_scan(root, generation, Some("directory unreadable"))
            .await
            .unwrap());
        let failed = db.load_folder_scan_snapshots().await.unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].items.len(), 2);
        assert!(matches!(
            &failed[0].status,
            crate::import::FolderScanStatus::Failed { error }
                if error == "directory unreadable"
        ));

        let generation = db.begin_folder_scan(root).await.unwrap();
        assert!(db
            .save_folder_scan_item(root, generation, &second, &[])
            .await
            .unwrap());
        assert!(db.finish_folder_scan(root, generation, None).await.unwrap());
        let complete = db.load_folder_scan_snapshots().await.unwrap();
        assert_eq!(complete[0].items.len(), 1);
        assert_eq!(complete[0].items[0].persisted_key(), second.persisted_key());
        assert_eq!(
            complete[0].status,
            crate::import::FolderScanStatus::Complete
        );

        assert!(
            !db.save_folder_scan_item(root, generation - 1, &first, &[])
                .await
                .unwrap(),
            "a superseded generation cannot overwrite the stored snapshot"
        );
    }

    #[tokio::test]
    async fn folder_scan_item_rejects_a_mismatched_embedded_root_without_changing_the_snapshot() {
        let (db, _tmp) = empty_db().await;
        let root = &host_root("/mounted/library");
        db.add_watched_import_folder(root).await.unwrap();
        let generation = db.begin_folder_scan(root).await.unwrap();
        let existing = scanned_candidate(root, "Existing");
        db.save_folder_scan_item(root, generation, &existing, &[])
            .await
            .unwrap();

        let mismatched = scanned_candidate(&host_root("/other/library"), "Injected");
        let error = db
            .save_folder_scan_item(root, generation, &mismatched, &[])
            .await
            .unwrap_err();

        assert!(error.to_string().contains("does not belong"));
        let snapshot = db.load_folder_scan_snapshots().await.unwrap();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].items.len(), 1);
        assert_eq!(
            snapshot[0].items[0].persisted_key(),
            existing.persisted_key()
        );
    }

    #[tokio::test]
    async fn imported_content_hash_lookup_uses_its_partial_index() {
        let (db, _tmp) = empty_db().await;
        let plan = db
            .handle()
            .sql_read(move |sql| {
                let details = sql.query(
                    "EXPLAIN QUERY PLAN \
                     SELECT 1 FROM releases WHERE content_hash = ? LIMIT 1",
                    ["hash"],
                    |row| row.get::<_, String>(3),
                )?;
                Ok::<_, coven::CovenError>(details)
            })
            .await
            .unwrap();

        assert!(
            plan.iter()
                .any(|detail| detail.contains("idx_releases_content_hash")),
            "query plan did not use the content-hash index: {plan:?}"
        );
    }

    #[tokio::test]
    async fn folder_decisions_remove_contradictory_scan_rows_before_failed_rescan() {
        use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

        let (db, _tmp) = empty_db().await;
        let root = &host_root("/mounted/library");
        db.add_watched_import_folder(root).await.unwrap();
        let generation = db.begin_folder_scan(root).await.unwrap();
        for name in ["Box/CD1", "Box/CD2"] {
            let item = scanned_candidate(root, name);
            db.save_folder_scan_item(root, generation, &item, &[])
                .await
                .unwrap();
        }
        let key = FolderReleaseDecisionKey {
            watched_folder_path: root.to_string(),
            relative_folder_path: "Box".to_string(),
        };
        let (combine_generation, combine_removals) = db
            .set_folder_release_decisions(&[(
                key.clone(),
                FolderReleaseDecision::CombineAsOneRelease,
            )])
            .await
            .unwrap();
        assert_eq!(
            combine_removals,
            vec![
                scanned_candidate(root, "Box/CD1").persisted_key(),
                scanned_candidate(root, "Box/CD2").persisted_key(),
            ]
        );
        db.finish_folder_scan(root, combine_generation, Some("share disconnected"))
            .await
            .unwrap();
        assert!(db.load_folder_scan_snapshots().await.unwrap()[0]
            .items
            .is_empty());
        assert_eq!(
            db.load_folder_release_decisions(root)
                .await
                .unwrap()
                .get("Box"),
            Some(FolderReleaseDecision::CombineAsOneRelease)
        );

        let generation = db.begin_folder_scan(root).await.unwrap();
        db.save_folder_scan_item(root, generation, &scanned_candidate(root, "Box"), &[])
            .await
            .unwrap();
        let (separate_generation, separate_removals) = db
            .set_folder_release_decisions(&[(key, FolderReleaseDecision::KeepAsSeparateReleases)])
            .await
            .unwrap();
        assert_eq!(
            separate_removals,
            vec![scanned_candidate(root, "Box").persisted_key()]
        );
        db.finish_folder_scan(root, separate_generation, Some("share disconnected"))
            .await
            .unwrap();
        assert!(db.load_folder_scan_snapshots().await.unwrap()[0]
            .items
            .is_empty());
        assert_eq!(
            db.load_folder_release_decisions(root)
                .await
                .unwrap()
                .get("Box"),
            Some(FolderReleaseDecision::KeepAsSeparateReleases)
        );
    }

    #[tokio::test]
    async fn removed_and_readded_root_rejects_items_from_its_old_registration() {
        let (db, _tmp) = empty_db().await;
        let root = &host_root("/mounted/library");
        db.add_watched_import_folder(root).await.unwrap();
        let old_generation = db.begin_folder_scan(root).await.unwrap();

        db.remove_watched_import_folder(root).await.unwrap();
        db.add_watched_import_folder(root).await.unwrap();
        let new_generation = db.begin_folder_scan(root).await.unwrap();
        assert!(new_generation > old_generation);

        assert!(!db
            .save_folder_scan_item(root, old_generation, &scanned_candidate(root, "Old"), &[],)
            .await
            .unwrap());
        assert!(db.load_folder_scan_snapshots().await.unwrap()[0]
            .items
            .is_empty());
    }

    #[tokio::test]
    async fn folder_decision_failure_rolls_back_decision_entries_and_generation() {
        use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

        let (db, _tmp) = empty_db().await;
        let root = &host_root("/mounted/library");
        db.add_watched_import_folder(root).await.unwrap();
        let generation = db.begin_folder_scan(root).await.unwrap();
        db.save_folder_scan_item(root, generation, &scanned_candidate(root, "Box/CD1"), &[])
            .await
            .unwrap();
        db.call(|conn| {
            conn.execute(
                "UPDATE folder_scan_generation_sequence SET last_generation = ?",
                [i64::MAX],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let result = db
            .set_folder_release_decisions(&[(
                FolderReleaseDecisionKey {
                    watched_folder_path: root.to_string(),
                    relative_folder_path: "Box".to_string(),
                },
                FolderReleaseDecision::CombineAsOneRelease,
            )])
            .await;
        assert!(result.is_err());

        assert!(db
            .load_folder_release_decisions(root)
            .await
            .unwrap()
            .get("Box")
            .is_none());
        let snapshots = db.load_folder_scan_snapshots().await.unwrap();
        assert_eq!(snapshots[0].generation, generation);
        assert_eq!(snapshots[0].items.len(), 1);
        assert_eq!(
            snapshots[0].items[0].persisted_key(),
            scanned_candidate(root, "Box/CD1").persisted_key()
        );
    }

    #[tokio::test]
    async fn removing_watched_root_cascades_all_local_folder_state() {
        let (db, _tmp) = empty_db().await;
        let root = &host_root("/mounted/library");
        db.add_watched_import_folder(root).await.unwrap();
        db.set_import_candidate_skipped(root, "Collection/Release", true)
            .await
            .unwrap();
        db.set_folder_release_decision(
            &crate::import::folder_scanner::FolderReleaseDecisionKey {
                watched_folder_path: root.to_string(),
                relative_folder_path: "Collection".to_string(),
            },
            crate::import::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases,
        )
        .await
        .unwrap();
        let generation = db.begin_folder_scan(root).await.unwrap();
        db.save_folder_scan_item(root, generation, &scanned_candidate(root, "Release"), &[])
            .await
            .unwrap();

        assert!(db.remove_watched_import_folder(root).await.unwrap());
        assert!(db
            .load_import_folder_registry()
            .await
            .unwrap()
            .watched_folders()
            .is_empty());
        assert_eq!(
            db.load_folder_release_decisions(root)
                .await
                .unwrap()
                .get("Collection"),
            None
        );
        assert!(db.load_folder_scan_snapshots().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn watched_root_overlap_uses_paths_not_sql_patterns() {
        let (db, _tmp) = empty_db().await;
        for root in ["/music/100%", "/music/name_value"] {
            assert!(db
                .add_watched_import_folder(&host_root(root))
                .await
                .unwrap());
        }
        for child in ["/music/100%/child", "/music/name_value/child"] {
            let error = db
                .add_watched_import_folder(&host_root(child))
                .await
                .unwrap_err();
            assert!(error.to_string().contains("cannot overlap"), "{child}");
        }
    }

    #[tokio::test]
    async fn watched_root_order_survives_middle_removal_and_later_add() {
        let (db, _tmp) = empty_db().await;
        for root in ["/one", "/two", "/three"] {
            db.add_watched_import_folder(&host_root(root))
                .await
                .unwrap();
        }
        db.remove_watched_import_folder(&host_root("/two"))
            .await
            .unwrap();
        db.add_watched_import_folder(&host_root("/four"))
            .await
            .unwrap();
        let paths: Vec<_> = db
            .load_import_folder_registry()
            .await
            .unwrap()
            .watched_folders()
            .into_iter()
            .map(|folder| folder.path)
            .collect();
        assert_eq!(
            paths,
            vec![host_root("/one"), host_root("/three"), host_root("/four")]
        );
    }

    /// However the folder was spelled on the way in, one row exists and it is
    /// keyed by the canonical spelling — so a second spelling of a folder
    /// already watched is recognized as the same folder rather than added
    /// beside it.
    #[tokio::test]
    async fn watched_root_spellings_settle_on_one_row() {
        let (db, _tmp) = empty_db().await;
        let canonical = host_root("/music/rips");
        assert!(db.add_watched_import_folder(&canonical).await.unwrap());

        // The last of these is the drive-lettered, forward-slashed form a
        // `bae://import` link and a `file://` folder drop hand over on Windows.
        #[cfg(windows)]
        const URL_SPELLINGS: &[&str] = &["C:/music/rips"];
        #[cfg(not(windows))]
        const URL_SPELLINGS: &[&str] = &[];

        let spellings = [
            host_root("/music/rips/"),
            host_root("/music//rips"),
            host_root("/music/./rips"),
        ];

        for spelling in spellings
            .iter()
            .map(String::as_str)
            .chain(URL_SPELLINGS.iter().copied())
        {
            assert!(
                !db.add_watched_import_folder(spelling).await.unwrap(),
                "{spelling} is the folder already watched, not a new one"
            );
        }
        let paths: Vec<_> = db
            .load_import_folder_registry()
            .await
            .unwrap()
            .watched_folders()
            .into_iter()
            .map(|folder| folder.path)
            .collect();
        assert_eq!(paths, vec![canonical]);
    }

    /// `..` never becomes a key: rewriting it without reading the filesystem
    /// is wrong across a symlink, so it is refused instead.
    #[tokio::test]
    async fn watched_root_rejects_a_path_climbing_out_of_itself() {
        let (db, _tmp) = empty_db().await;
        let path = host_root("/music/../rips");
        assert!(db.add_watched_import_folder(&path).await.is_err(), "{path}");
    }

    #[tokio::test]
    async fn corrupt_relative_folder_keys_fail_when_loaded() {
        let (db, _tmp) = empty_db().await;
        let root = host_root("/mounted/library");
        db.add_watched_import_folder(&root).await.unwrap();
        assert!(db
            .set_import_candidate_skipped(&root, "a//b", true)
            .await
            .is_err());
        assert!(db
            .set_folder_release_decision(
                &crate::import::folder_scanner::FolderReleaseDecisionKey {
                    watched_folder_path: root.clone(),
                    relative_folder_path: "a/./b".to_string(),
                },
                crate::import::folder_scanner::FolderReleaseDecision::CombineAsOneRelease,
            )
            .await
            .is_err());
        let stored_root = root.clone();
        db.call(move |conn| {
            conn.execute(
                "INSERT INTO skipped_import_candidates VALUES (?, 'a//b')",
                params![stored_root],
            )?;
            conn.execute(
                "INSERT INTO folder_release_decisions VALUES (?, 'a/./b', 'combine_as_one_release')",
                params![stored_root],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        assert!(db.load_import_folder_registry().await.is_err());
        assert!(db.load_folder_release_decisions(&root).await.is_err());
    }

    #[tokio::test]
    async fn corrupt_scan_entry_identity_and_generation_fail_when_loaded() {
        let (db, _tmp) = empty_db().await;
        let root = &host_root("/mounted/library");
        db.add_watched_import_folder(root).await.unwrap();
        let generation = db.begin_folder_scan(root).await.unwrap();
        let item = scanned_candidate(root, "Release");
        db.save_folder_scan_item(root, generation, &item, &[])
            .await
            .unwrap();
        // A key naming a folder the stored item does not: the entry no longer
        // identifies its own item.
        let other_key = scanned_candidate(root, "Other").persisted_key();
        db.call(move |conn| {
            conn.execute(
                "UPDATE folder_scan_entries SET entry_key = ?",
                params![other_key],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        assert!(db.load_folder_scan_snapshots().await.is_err());

        // Key restored, so only the generation is now wrong.
        let item_key = item.persisted_key();
        db.call(move |conn| {
            conn.execute(
                "UPDATE folder_scan_entries SET entry_key = ?, generation = ?",
                params![item_key, i64::try_from(generation + 1).unwrap()],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        assert!(db.load_folder_scan_snapshots().await.is_err());
    }

    /// A disc assignment the user set survives a relaunch: it is stored under
    /// the candidate's content hash, read back from a cold database, and the
    /// scan that follows lays the discs down as they settled them rather than
    /// in the order the cue filenames read.
    #[tokio::test]
    async fn a_disc_assignment_survives_a_relaunch() {
        use crate::import::folder_scanner::{
            collect_release_candidate_files_with_scope, CandidateFileEdits, SheetDisc,
            SheetDiscEdits, StoredCandidateEdits,
        };

        let (db, _tmp) = empty_db().await;
        let folder = two_sheet_folder();
        let scanned = collect_release_candidate_files_with_scope(
            folder.path(),
            crate::import::ReleaseFileScope::Recursive,
            &StoredCandidateEdits::none(),
        )
        .unwrap();
        let root = folder.path().to_string_lossy().into_owned();
        db.add_watched_import_folder(&root).await.unwrap();
        let generation = db.begin_folder_scan(&root).await.unwrap();
        let candidate = crate::import::folder_scanner::FolderCandidate {
            path: folder.path().to_path_buf(),
            file_root: folder.path().to_path_buf(),
            name: "Release".to_string(),
            files: scanned.clone(),
            watched_folder_path: root.clone(),
            scope: crate::import::ReleaseFileScope::Recursive,
            file_edit_revision: 0,
            display_path: String::new(),
            resolved_boundaries: Vec::new(),
            combine_ancestor_key: None,
        };
        db.save_folder_scan_item(
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(candidate),
            &[],
        )
        .await
        .unwrap();
        db.finish_folder_scan(&root, generation, None)
            .await
            .unwrap();

        // The rip named its sheets the other way round: `alpha.cue` is disc two.
        let mut sheet_discs = SheetDiscEdits::default();
        sheet_discs.set("alpha.cue".to_string(), SheetDisc::Disc { number: 2 });
        sheet_discs.set("beta.cue".to_string(), SheetDisc::Disc { number: 1 });
        let candidate_edits = CandidateFileEdits {
            sheet_discs,
            ..Default::default()
        };
        let mut settled = scanned.clone();
        settled
            .apply_candidate_file_edits(&candidate_edits)
            .unwrap();
        db.save_import_candidate_file_edits(
            &scanned.content_hash(),
            &folder.path().to_string_lossy(),
            0,
            &candidate_edits,
            &[(folder.path().to_string_lossy().into_owned(), settled)],
        )
        .await
        .unwrap();

        let current = db
            .load_candidate_file_edits(&scanned.content_hash())
            .await
            .unwrap();
        assert_eq!(current.revision, 1);
        assert_eq!(
            current.sheet_discs.get("alpha.cue"),
            Some(SheetDisc::Disc { number: 2 })
        );

        // A subsequent scan reads the same decisions, so the folder's audio
        // comes out in the order the user settled rather than in path order.
        let stored = db.load_stored_candidate_edits().await.unwrap();
        let reopened = collect_release_candidate_files_with_scope(
            folder.path(),
            crate::import::ReleaseFileScope::Recursive,
            &stored,
        )
        .unwrap();
        assert_eq!(
            reopened
                .carving_sheets()
                .iter()
                .map(|sheet| (sheet.file.relative_path.as_str(), sheet.disc))
                .collect::<Vec<_>>(),
            vec![
                ("alpha.cue", SheetDisc::Disc { number: 2 }),
                ("beta.cue", SheetDisc::Disc { number: 1 }),
            ],
        );
    }

    /// Two bound single-track sheets, each naming the audio beside it.
    fn two_sheet_folder() -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for stem in ["alpha", "beta"] {
            std::fs::copy(
                fixtures.join("tests/fixtures/cue_flac/Test Album.flac"),
                tmp.path().join(format!("{stem}.flac")),
            )
            .unwrap();
            std::fs::write(
                tmp.path().join(format!("{stem}.cue")),
                format!(
                    "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n\
                     FILE \"{stem}.flac\" WAVE\n  TRACK 01 AUDIO\n    \
                     TITLE \"Track Title\"\n    INDEX 01 00:00:00\n",
                ),
            )
            .unwrap();
        }
        tmp
    }

    /// The walkthrough folder on disk: a twelve-track sheet written against a
    /// WAV, the FLAC it was actually encoded to, and the rip log.
    fn walkthrough_folder() -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        std::fs::copy(
            fixtures.join("tests/fixtures/cue_flac/Test Album.flac"),
            tmp.path().join("cd.flac"),
        )
        .unwrap();
        std::fs::copy(
            fixtures.join("tests/fixtures/test_album.log"),
            tmp.path().join("rip.log"),
        )
        .unwrap();
        let mut cue =
            String::from("PERFORMER \"Test Artist\"\nTITLE \"Album\"\nFILE \"cd.wav\" WAVE\n");
        for track in 1..=12 {
            cue.push_str(&format!(
                "  TRACK {track:02} AUDIO\n    TITLE \"Track {track:02}\"\n    INDEX 01 {:02}:00:00\n",
                (track - 1) * 5,
            ));
        }
        std::fs::write(tmp.path().join("cd.cue"), cue).unwrap();
        tmp
    }
}

/// The ids this layer mints itself — a release's `release_identities` rows, and
/// the `album_artists` rows copied when a `set_identity` moves a release to a new
/// album — come from the injected [`coven::IdProvider`], like every other id in
/// the process. Minting them with a raw `Uuid::new_v4()` would put an id source
/// nobody injected inside the DB, and a test running a deterministic provider
/// would still get random ones.
#[cfg(test)]
mod injected_ids_tests {
    use super::super::*;
    use super::*;
    use crate::db::{DbAlbum, DbAlbumArtist, DbArtist, DbRelease, ReleaseMetadataSource};
    use crate::import::{MetadataSource, ReleaseIdentity};
    use chrono::Utc;
    use coven::SystemClock;
    use std::sync::Arc;

    /// A deterministic id provider yielding valid RFC4122 v4 UUIDs from a counter,
    /// so a `Database` built for a test satisfies coven's `IndependentUuid`
    /// row-identity validation (it rejects a non-UUID id in a synced-table write)
    /// while staying stable and greppable across runs. The fixed `bae0da7a` prefix
    /// marks an id as minted by this test provider.
    struct SequentialUuidProvider {
        next: std::sync::atomic::AtomicU64,
    }

    impl coven::IdProvider for SequentialUuidProvider {
        fn new_id(&self) -> String {
            let n = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            format!("bae0da7a-0000-4000-8000-{n:012x}")
        }
    }

    async fn db_with_sequential_ids() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            Arc::new(SequentialUuidProvider {
                next: std::sync::atomic::AtomicU64::new(1),
            }),
        )
        .await
        .unwrap();
        (db, tmp)
    }

    fn identity(source: MetadataSource, release_id: &str) -> ReleaseIdentity {
        ReleaseIdentity {
            source,
            source_group_id: format!("group-{release_id}"),
            source_release_id: Some(release_id.to_string()),
        }
    }

    #[tokio::test]
    async fn identity_rows_take_their_ids_from_the_injected_provider() {
        let (db, _tmp) = db_with_sequential_ids().await;
        let now = Utc::now();

        let artist = DbArtist {
            id: ARTIST_1.to_string(),
            name: "Artist".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        db.insert_artist(&artist).await.unwrap();

        let mut album = DbAlbum::new_test("Album", &artist.id);
        album.id = ALBUM_1.to_string();
        db.insert_album(&album).await.unwrap();
        db.insert_album_artist(&DbAlbumArtist::new(
            &album.id,
            &artist.id,
            0,
            ALBUM_ARTIST_1.to_string(),
            now,
        ))
        .await
        .unwrap();

        let release = DbRelease::new_test(&album.id, RELEASE_1);
        db.insert_release(&release).await.unwrap();

        db.insert_release_identities(
            &release.id,
            &[identity(MetadataSource::Discogs, "discogs-release-1")],
        )
        .await
        .unwrap();

        let ids: Vec<String> = db
            .read(|sql| {
                sql.query("SELECT id FROM release_identities", [], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(DbError::from)
            })
            .await
            .unwrap();

        assert_eq!(ids.len(), 1, "one identity row was written, got {ids:?}",);
        assert!(
            ids[0].starts_with("bae0da7a-"),
            "the identity row's id comes from the injected provider, got {:?}",
            ids[0],
        );

        // Moving the release to a fresh album copies its album_artists rows; those
        // PKs are minted here too.
        let target = DbAlbum::new_test("Target Album", &artist.id);
        db.set_identity_atomic(
            &release.id,
            &[identity(MetadataSource::MusicBrainz, "mb-release-1")],
            ReleaseMetadataSource::MusicBrainz,
            Some("mb-release-1"),
            &album.id,
            &target.id,
            Some(&target),
        )
        .await
        .unwrap();

        let copied: Vec<String> = db
            .read(move |sql| {
                sql.query("SELECT id FROM album_artists WHERE album_id != '9644b84d-94b2-4b3b-863a-d6583931920c'", [], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(DbError::from)
            })
            .await
            .unwrap();

        assert_eq!(
            copied.len(),
            1,
            "the album_artists row was copied, got {copied:?}"
        );
        assert!(
            copied[0].starts_with("bae0da7a-"),
            "the copied album_artists row's id comes from the injected provider, got {:?}",
            copied[0],
        );
    }
}

/// A queue row's cover is the cover of the release the track is actually on —
/// never the album's `primary_release_id`, which is nullable (a fresh album from
/// `set_identity`, or an album whose chosen release was deleted) and which points
/// at a *different* release than the queued track whenever the album has more than
/// one.
#[cfg(test)]
mod queue_cover_tests {
    use super::super::*;
    use super::*;
    use crate::playback::QueueEntryId;
    use coven::SystemClock;
    use std::sync::Arc;

    /// `album-null` has no primary release at all; `album-set` has one, pointing at
    /// a release that the queued track is NOT on.
    async fn cover_db() -> (Database, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::new_test(
            path.to_str().unwrap(),
            Arc::new(SystemClock),
            Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        // coven verifies a blob row's declared hash, so the seeded covers carry
        // real content hashes rather than placeholder strings.
        let lonely_hash = crate::util::fs::hash_bytes(b"cover-lonely");
        let other_hash = crate::util::fs::hash_bytes(b"cover-other");
        db.call(move |conn| {
            conn.execute_batch(
                &format!(
                "
                INSERT INTO artists (id, name, _updated_at, created_at)
                VALUES ('d7d8141f-54ff-467d-8b60-4f34a4d2e528', 'Artist Name', 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
                VALUES
                    ('c6648d5a-617e-4b69-87da-b7f1c4fb5e65', 'Album With No Primary', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, NULL, 0, 'stamp', '2026-01-01T00:00:00Z'),
                    ('82a53f44-1b76-435b-89f0-42749371ee15', 'Album With A Primary', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, '2ffa8060-aa00-4147-8ad4-f373ef66c407', 0, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
                VALUES
                    ('fcf4be32-159f-4790-87a1-697700a74462', 'c6648d5a-617e-4b69-87da-b7f1c4fb5e65', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('2ffa8060-aa00-4147-8ad4-f373ef66c407', '82a53f44-1b76-435b-89f0-42749371ee15', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                    ('ce596bd7-be97-4416-8b6d-47f315bae466', '82a53f44-1b76-435b-89f0-42749371ee15', 'file_tags', 1, 'stamp', '2026-01-01T00:00:01Z');

                INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
                VALUES
                    ('03c41035-ce18-4fa0-8e83-c446df26a551', 'fcf4be32-159f-4790-87a1-697700a74462', 'Track On The Only Release', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                    ('69e67928-545a-4dcf-8ae7-ef7778331231', 'ce596bd7-be97-4416-8b6d-47f315bae466', 'Track On The Non-Primary Release', 1, 1, 1000, NULL, 'stamp', '2026-01-01T00:00:00Z');

                INSERT INTO covers (id, blob_id, content_type, file_size, source, hash, _updated_at, created_at)
                VALUES
                    ('fcf4be32-159f-4790-87a1-697700a74462', 'bd5c1f6c-3b6e-4d16-9f0a-2c1d5f61a0aa', 'image/jpeg', 1024, 'discogs', '{lonely_hash}', 'cover-stamp-lonely', '2026-01-01T00:00:00Z'),
                    ('ce596bd7-be97-4416-8b6d-47f315bae466', '0f2b9a51-7d2c-4a2f-8f16-9c0a3f1b2d44', 'image/jpeg', 1024, 'discogs', '{other_hash}', 'cover-stamp-other', '2026-01-01T00:00:00Z');
                "
                ),
            )
            .map(|_| ())
            .map_err(DbError::from)
        })
        .await
        .unwrap();
        (db, tmp)
    }

    async fn cover_of(db: &Database, track_id: &str) -> Option<crate::album_detail::ImageRef> {
        let items = db
            .get_queue_items(&[QueueEntry {
                id: QueueEntryId(format!("entry-{track_id}")),
                track_id: track_id.to_string(),
            }])
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        items[0].cover_image.clone()
    }

    fn cover_ref(release_id: &str, version: &str) -> Option<crate::album_detail::ImageRef> {
        Some(crate::album_detail::ImageRef {
            id: release_id.to_string(),
            version: version.to_string(),
            image_type: LibraryImageType::Cover,
        })
    }

    /// `albums.primary_release_id` is NULL, so reading it raw yields no cover at
    /// all and the queue row renders art-less.
    #[tokio::test]
    async fn queue_row_covers_a_track_whose_album_has_no_primary_release() {
        let (db, _tmp) = cover_db().await;
        assert_eq!(
            cover_of(&db, TRACK_LONELY).await,
            cover_ref(RELEASE_LONELY, "cover-stamp-lonely"),
        );
    }

    /// The album's primary release is set, but the queued track is on a different
    /// one — the row must show the art of the release being played.
    #[tokio::test]
    async fn queue_row_covers_the_track_s_own_release_not_the_album_s_primary() {
        let (db, _tmp) = cover_db().await;
        assert_eq!(
            cover_of(&db, TRACK_OTHER).await,
            cover_ref(RELEASE_OTHER, "cover-stamp-other"),
        );
    }
}
