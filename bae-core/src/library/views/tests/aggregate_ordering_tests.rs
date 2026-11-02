//! The views' aggregates keep their parts in order: credits, release ids,
//! composers and track artists inside each row, and a release's files and
//! index among its album's releases.

use crate::db::client::tests::aggregate_ordering_tests::*;

use crate::db::client::tests::*;

use crate::library::views::*;

#[tokio::test]
async fn album_summary_orders_artist_names_and_release_ids_inside_aggregates() {
    let (db, _tmp) = aggregate_db().await;
    // The one album in the fixture, read the way the library page reads it.
    let summary = crate::library::views::LibraryViews::new(db.clone())
        .get_album_page(&[], 0, 1)
        .await
        .unwrap()
        .pop()
        .expect("album summary row");

    assert_eq!(summary.artist_names, "Artist Name Primary, Artist Name First, Artist Name Second");
    assert_eq!(summary.release_ids, vec![RELEASE_Z, RELEASE_A, RELEASE_B]);
}

#[tokio::test]
async fn work_summary_orders_composer_names_inside_the_aggregate() {
    let (db, _tmp) = aggregate_db().await;
    let results = crate::library::views::LibraryViews::new(db.clone())
        .search_rows("Work Title A", 10)
        .await
        .unwrap();
    let summary = results.works.first().expect("work summary row");

    assert_eq!(
        summary.composer_names.as_deref(),
        Some("Composer Name First, Composer Name Second")
    );
}

#[tokio::test]
async fn storage_page_orders_album_aggregate_columns_inside_aggregates() {
    let (db, _tmp) = aggregate_db().await;
    let sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    let rows = crate::library::views::LibraryViews::new(db.clone())
        .get_storage_page(&sort, StorageFilter::All, 0, 10)
        .await
        .unwrap()
        .rows;
    let row = rows.first().expect("storage row");

    assert_eq!(
        row.album.artist_names,
        "Artist Name Primary, Artist Name First, Artist Name Second"
    );
    assert_eq!(row.album.release_ids, vec![RELEASE_Z, RELEASE_A, RELEASE_B]);
}

#[tokio::test]
async fn album_and_storage_pages_list_credits_in_order_and_allow_none() {
    let (db, _tmp) = crate::db::client::tests::temp_db().await;
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, name_key, created_at) VALUES ('7e7d8df5-8292-4287-80be-7abd24f5a992', 'Artist Name First', 'artist name first', '2026-01-01T00:00:00Z'),
             ('863911c6-a6b6-40a7-8096-b85eb877f7c7', 'Artist Name Second', 'artist name second', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES ('f6506bc5-0b41-44de-862f-1668e72c08c6', 'Album Title Empty', 2026, 0, '2026-01-01T00:00:00Z'),
             ('049faa5b-52d9-4109-832b-f6853740c876', 'Album Title Extra', 2026, 0, '2026-01-01T00:00:00Z');

            INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('c9966a2c-61b1-40dd-87ef-118587e57fe7', '049faa5b-52d9-4109-832b-f6853740c876', '863911c6-a6b6-40a7-8096-b85eb877f7c7', 1, '2026-01-01T00:00:00Z'),
             ('4953a209-c6c7-4b0c-82dd-cc13e47af890', '049faa5b-52d9-4109-832b-f6853740c876', '7e7d8df5-8292-4287-80be-7abd24f5a992', 0, '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('7c3d0881-e6d0-4252-8075-709b2282bcc1', 'f6506bc5-0b41-44de-862f-1668e72c08c6', '2026-01-01T00:00:00Z', 0),
             ('ba00ebe0-da50-428a-8ceb-2389d9a9f232', '049faa5b-52d9-4109-832b-f6853740c876', '2026-01-01T00:00:00Z', 1);
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let album_sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    let albums = crate::library::views::LibraryViews::new(db.clone())
        .get_album_page(&album_sort, 0, 10)
        .await
        .unwrap();
    assert_eq!(albums.len(), 2);
    assert_eq!(albums[0].artist_names, "");
    assert_eq!(albums[1].artist_names, "Artist Name First, Artist Name Second");

    let storage_sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    let rows = crate::library::views::LibraryViews::new(db.clone())
        .get_storage_page(&storage_sort, StorageFilter::All, 0, 10)
        .await
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].album.artist_names, "");
    assert_eq!(rows[1].album.artist_names, "Artist Name First, Artist Name Second");
}

#[tokio::test]
async fn release_detail_orders_track_artists_and_keeps_tracks_without_artists() {
    let (db, _tmp) = release_detail_db().await;
    let detail = crate::library::views::LibraryViews::new(db.clone())
        .find_db_release_detail(RELEASE_A)
        .await
        .unwrap()
        .expect("release detail");

    let track_ids: Vec<&str> = detail.tracks.iter().map(|track| track.track.id.as_str()).collect();
    assert_eq!(track_ids, vec![TRACK_A, TRACK_B]);

    let artist_names: Vec<&str> =
        detail.tracks[0].artists.iter().map(|artist| artist.artist.name.as_str()).collect();
    assert_eq!(artist_names, vec!["Track Artist Name First", "Track Artist Name Second"]);
    assert!(detail.tracks[1].artists.is_empty());
}

/// A release's files come back in each order the Storage sheet sorts them
/// by — name, size, or kind, each forward or backward, ties by name and then
/// id — from the read itself, which walks that order's index either way. The
/// read follows a new order or release in place.
#[tokio::test]
async fn release_files_are_read_in_each_order_the_storage_sheet_sorts_by() {
    use crate::library::{ReleaseFileColumn, ReleaseFileOrder, ReleaseFilesRequest};
    let (db, tmp) = release_detail_db().await;
    for (i, (name, size, mime)) in [
        ("Track 10.flac", 30, "audio/flac"),
        ("cover.jpg", 20, "image/jpeg"),
        ("Track 2.flac", 30, "audio/flac"),
        ("album.cue", 10, "application/x-cue"),
    ]
    .into_iter()
    .enumerate()
    {
        insert_file(
            &db,
            tmp.path(),
            NewReleaseFile {
                id: bae_test_support::test_uuid(&format!("sorted-file-{i}")),
                release_id: RELEASE_A.to_string(),
                original_filename: name.to_string(),
                content_type: bae_util::content_type::ContentType::from_mime(mime),
                source_audio: None,
                created_at: chrono::Utc::now(),
            },
            size,
        )
        .await;
    }
    let order = |column, descending| ReleaseFileOrder { column, descending };
    let request = |order| Some(ReleaseFilesRequest { release_id: RELEASE_A.to_string(), order });
    // The names of the first files the read gives for `asked`.
    async fn names(
        files: &mut coven::ReconfigurableLiveQuery<
            Option<ReleaseFilesRequest>,
            Option<Vec<DbFile>>,
        >,
        asked: &Option<ReleaseFilesRequest>,
    ) -> Vec<String> {
        loop {
            let event = files.next().await.expect("the read runs");
            if event.request == *asked {
                break event
                    .result
                    .unwrap()
                    .expect("the release exists")
                    .into_iter()
                    .map(|file| file.original_filename)
                    .collect();
            }
        }
    }
    let first = request(order(ReleaseFileColumn::Name, false));
    let mut files =
        crate::library::views::LibraryViews::new(db.clone()).subscribe_release_files(first.clone());
    assert_eq!(
        names(&mut files, &first).await,
        ["album.cue", "cover.jpg", "Track 2.flac", "Track 10.flac"]
    );
    for (column, descending, expected, index) in [
        (
            ReleaseFileColumn::Name,
            true,
            ["Track 10.flac", "Track 2.flac", "cover.jpg", "album.cue"],
            Some("idx_release_files_release_order"),
        ),
        (
            ReleaseFileColumn::Size,
            false,
            ["album.cue", "cover.jpg", "Track 2.flac", "Track 10.flac"],
            None,
        ),
        (
            ReleaseFileColumn::Size,
            true,
            ["Track 10.flac", "Track 2.flac", "cover.jpg", "album.cue"],
            None,
        ),
        (
            ReleaseFileColumn::Kind,
            false,
            ["album.cue", "Track 2.flac", "Track 10.flac", "cover.jpg"],
            Some("idx_release_files_release_kind"),
        ),
        (
            ReleaseFileColumn::Kind,
            true,
            ["cover.jpg", "Track 10.flac", "Track 2.flac", "album.cue"],
            Some("idx_release_files_release_kind"),
        ),
    ] {
        let asked = request(order(column, descending));
        files.requests().set(asked.clone()).unwrap();
        assert_eq!(
            names(&mut files, &asked).await,
            expected,
            "{column:?} descending: {descending}"
        );
        let statement =
            crate::library::views::release_files::release_files_in(order(column, descending));
        let plan = db
            .read(move |sql| {
                sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), [RELEASE_A], |row| {
                    row.get::<_, String>(3)
                })
                .map_err(DbError::from)
            })
            .await
            .unwrap();
        match index {
            Some(index) => assert!(
                plan.iter().any(|step| step.contains(index))
                    && !plan.iter().any(|step| step.contains("TEMP B-TREE")),
                "{column:?} descending: {descending} walks {index}: {plan:?}"
            ),
            // Sizes are coven's: the release's few files are found by an
            // index of the release's and sorted by their coven files' sizes.
            None => assert!(
                plan.iter()
                    .any(|step| step.starts_with("SEARCH f USING") && step.contains("release_id=?")),
                "{column:?} descending: {descending} finds the release's files by index: {plan:?}"
            ),
        }
    }
}

/// A release's index among its album's releases is how many come before it
/// in the album's order, by place and then id, counted on the album's
/// releases index.
#[tokio::test]
async fn a_releases_index_counts_the_releases_before_it() {
    let (db, _tmp) = aggregate_db().await;
    // Places 0, 1, 1: the two at place 1 go by id.
    for (release_id, index) in [
        ("8aa66d48-65a0-42e4-8c1d-e7481e8c1861", 0),
        ("0252dedb-ee39-4547-8803-438dbeb57a64", 1),
        ("64e79a1f-404a-4c34-809a-a3cb44bf1942", 2),
    ] {
        let context = crate::library::views::LibraryViews::new(db.clone())
            .find_release_detail_context(release_id)
            .await
            .unwrap()
            .expect("the release exists");
        assert_eq!(context.release_index, index, "{release_id}");
    }

    let plan = db
        .read(|sql| {
            sql.query(
                &format!(
                    "EXPLAIN QUERY PLAN {}",
                    crate::library::views::release::RELEASE_INDEX_IN_ALBUM
                ),
                ["8aa66d48-65a0-42e4-8c1d-e7481e8c1861"],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("idx_releases_album_id")),
        "the count reads the album's releases index: {plan:?}"
    );
}
