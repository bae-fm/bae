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

            INSERT INTO works (id, title, work_type, musicbrainz_work_id, _updated_at, created_at)
            VALUES
                ('432c8996-8af0-43dc-868a-822a256f65c4', 'Work Title A', 'work', 'mb-work-a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('d866f97d-e57f-45e8-8c4e-f81ad8717882', 'Work Title B', 'work', 'mb-work-b', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('00e1ff99-c327-477d-846d-28d2f27fa004', 'Work Title C', 'work', 'mb-work-c', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

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

            INSERT INTO works (id, title, work_type, musicbrainz_work_id, _updated_at, created_at)
            VALUES
                ('1d446150-576e-479f-87a7-40ac7a511fa1', 'Work Title A1', 'work', 'mb-work-a1', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('6a32dca0-bf5b-4baa-829d-dc2ef531e763', 'Work Title A2', 'work', 'mb-work-a2', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('735e697c-f2ce-4512-806c-4f872446f6e6', 'Work Title B1', 'work', 'mb-work-b1', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('d5f30cc0-a35a-4294-851b-ce2d9c172d1c', 'Work Title B2', 'work', 'mb-work-b2', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('5dc2446b-5241-46e0-8be4-4325e06f1417', 'Work Title Solo', 'work', 'mb-work-solo', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

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
