//! Live queries woken by writes another device made, which sync applied to
//! this one.

use super::*;

#[test]
fn album_page_subscription_delivers_a_write_materialized_by_sync() {
    crate::sync::test_devices::run_two_device_test(async |devices| {
        let writer = devices.a().database();
        let reader = devices.b().database();
        let mut live = reader.subscribe_album_browse(&[], false, first_page());
        assert_eq!(
            live.next()
                .await
                .expect("the query stays open")
                .result
                .unwrap()
                .total_count,
            0
        );
        exec_batch(writer, &format!(
            "INSERT INTO artists (id, name, name_key, created_at)
             VALUES ('{ARTIST_ID}', 'Artist Name', 'artist name', '2026-01-01T00:00:00Z');
             INSERT INTO albums (id, title, is_compilation, created_at) VALUES ('{ALBUM_ID}', 'Synced Album', 0, '2026-01-01T00:00:00Z');
            INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('ec0d8e00-2141-4bae-8618-9d7ca8af03c4', '{ALBUM_ID}', '{ARTIST_ID}', 0, '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('{RELEASE_ID}', '{ALBUM_ID}', '2026-01-01T00:00:00Z', 0);"
        )).await;
        devices
            .settle("the remote album", |_, db| async move {
                db.find_album_by_id(ALBUM_ID).await.unwrap().is_some()
            })
            .await;
        let updated = tokio::time::timeout(Duration::from_secs(20), live.next())
            .await
            .unwrap()
            .expect("the query stays open")
            .result
            .unwrap();
        assert_eq!(updated.windows[0].rows[0].title, "Synced Album");
    });
}

/// The composer list on B is a local table B's triggers keep from the synced
/// rows: applying A's writes runs them, and B's live query delivers what they
/// wrote. A's work credit lists its composer; A's merge then lists the work
/// under the survivor alone.
#[test]
fn composer_browse_subscription_delivers_listings_kept_by_applied_writes() {
    crate::sync::test_devices::run_two_device_test(async |devices| {
        let writer = devices.a().database();
        let reader = devices.b().database();
        let sort = [ComposerSortCriterion {
            field: ComposerSortField::Name,
            direction: SortDirection::Ascending,
        }];
        let mut live = reader.subscribe_composer_browse(&sort, first_page());
        assert_eq!(
            live.next()
                .await
                .expect("the query stays open")
                .result
                .unwrap()
                .total_count,
            0
        );
        let listed = async |live: &mut coven::ReconfigurableLiveQuery<
            crate::library::LibraryPageWindows,
            ComposerBrowseProjection,
        >,
                            expected: Vec<(&str, i64)>| {
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let shown = live
                        .next()
                        .await
                        .expect("the query stays open")
                        .result
                        .unwrap();
                    let rows = shown.windows[0]
                        .rows
                        .iter()
                        .map(|row| (row.artist.id.clone(), row.work_count))
                        .collect::<Vec<_>>();
                    if rows
                        == expected
                            .iter()
                            .map(|(id, works)| (id.to_string(), *works))
                            .collect::<Vec<_>>()
                    {
                        return shown;
                    }
                }
            })
            .await
            .expect("B's composer browse delivers the listing A's write makes")
        };

        exec_batch(writer, &format!(
            "INSERT INTO artists (id, name, name_key, created_at) VALUES
                ('{COMPOSER_ID}', 'Composer Name First', 'composer name first', '2026-01-01T00:00:00Z'),
                ('{OTHER_COMPOSER_ID}', 'Composer Name Second', 'composer name second', '2026-01-01T00:00:00Z');
             INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES
                ('{COMPOSER_WORK_ID}', 'Work Title First', 'work', 'work-first', '2026-01-01T00:00:00Z'),
                ('{OTHER_COMPOSER_WORK_ID}', 'Work Title Second', 'work', 'work-second', '2026-01-01T00:00:00Z');
             INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES
                ('{COMPOSER_LINK_ID}', '{COMPOSER_WORK_ID}', '{COMPOSER_ID}', 0, 'file_tags', '2026-01-01T00:00:00Z'),
                ('{OTHER_COMPOSER_LINK_ID}', '{OTHER_COMPOSER_WORK_ID}', '{OTHER_COMPOSER_ID}', 0, 'file_tags', '2026-01-01T00:00:00Z');"
        )).await;
        devices
            .settle("the remote work credits", |_, db| async move {
                db.find_composer_detail(OTHER_COMPOSER_ID)
                    .await
                    .unwrap()
                    .is_some()
            })
            .await;
        let credited = listed(&mut live, vec![(COMPOSER_ID, 1), (OTHER_COMPOSER_ID, 1)]).await;
        assert_eq!(credited.total_count, 2);

        exec(
            writer,
            "INSERT INTO artist_merges (id, into_artist_id, created_at) VALUES (?1, ?2, '2026-01-01T00:00:00Z')",
            &[OTHER_COMPOSER_ID, COMPOSER_ID],
        )
        .await;
        devices
            .settle("the remote merge", |_, db| async move {
                db.find_composer_detail(OTHER_COMPOSER_ID)
                    .await
                    .unwrap()
                    .is_none()
            })
            .await;
        let merged = listed(&mut live, vec![(COMPOSER_ID, 2)]).await;
        assert_eq!(merged.total_count, 1);
    });
}

/// B's search tables are local, written by B's triggers as it applies A's
/// writes: an album A writes, then renames, is found on B by each title in
/// turn.
#[test]
fn library_search_finds_rows_applied_from_another_device() {
    crate::sync::test_devices::run_two_device_test(async |devices| {
        let writer = devices.a().database();
        let reader = devices.b().database();
        exec_batch(writer, &format!(
            "INSERT INTO artists (id, name, name_key, created_at)
             VALUES ('{ARTIST_ID}', 'Artist Name', 'artist name', '2026-01-01T00:00:00Z');
             INSERT INTO albums (id, title, is_compilation, created_at) VALUES ('{ALBUM_ID}', 'Écrin Album Title', 0, '2026-01-01T00:00:00Z');
             INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('b416679d-8454-4a3b-abc6-db8b32a35731', '{ALBUM_ID}', '{ARTIST_ID}', 0, '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('{RELEASE_ID}', '{ALBUM_ID}', '2026-01-01T00:00:00Z', 0);"
        )).await;
        devices
            .settle("the remote album", |_, db| async move {
                db.find_album_by_id(ALBUM_ID).await.unwrap().is_some()
            })
            .await;
        let found = reader.search_library("écrin", 10).await.unwrap();
        assert_eq!(
            found
                .albums
                .iter()
                .map(|album| album.id.as_str())
                .collect::<Vec<_>>(),
            vec![ALBUM_ID]
        );

        exec(
            writer,
            "UPDATE albums SET title = 'Renamed Album Title' WHERE id = ?1",
            &[ALBUM_ID],
        )
        .await;
        devices
            .settle("the remote rename", |_, db| async move {
                db.find_album_by_id(ALBUM_ID).await.unwrap().unwrap().title == "Renamed Album Title"
            })
            .await;
        assert!(reader
            .search_library("écrin", 10)
            .await
            .unwrap()
            .albums
            .is_empty());
        assert_eq!(
            reader
                .search_library("renamed album", 10)
                .await
                .unwrap()
                .albums
                .len(),
            1
        );
    });
}
