const LISTED_SURVIVOR: &str = "87afe278-5071-4811-a8fb-06ad2ed2d01c";
const LISTED_ABSORBED: &str = "cc0b10bd-7e72-4e7a-85f6-6c6d5d5dc691";
const LISTED_CREDITED: &str = "60b490bc-23a7-41c3-8d2f-fe49880c33a8";

/// Two releases of one track each, on albums of their own, the first track
/// performing nothing yet.
const LISTED_RELEASES: &str = "
    INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES
        ('b8f2ed98-6074-4a3f-87d8-19598807e767', 'Album Title One', 2026, 0, '2026-01-01T00:00:00Z'),
        ('bb2d8f2e-43ec-4e07-a430-0b9950b88d92', 'Album Title Two', 2026, 0, '2026-01-01T00:00:00Z');
    INSERT INTO releases (id, album_id, created_at, album_order) VALUES
        ('c4325183-800c-41da-9082-5d998f9a7ecd', 'b8f2ed98-6074-4a3f-87d8-19598807e767', '2026-01-01T00:00:00Z', 0),
        ('f3dbd598-48d1-44e8-9608-94db537ec89c', 'bb2d8f2e-43ec-4e07-a430-0b9950b88d92', '2026-01-01T00:00:00Z', 0);
    INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) VALUES
        ('6ac819de-8e61-419c-9db4-b1582fc9feda', 'c4325183-800c-41da-9082-5d998f9a7ecd', 1, 'cd', 1, '2026-01-01T00:00:00Z'),
        ('d0fb26fe-9cc5-4116-909d-5376f603c5b7', 'f3dbd598-48d1-44e8-9608-94db537ec89c', 1, 'cd', 1, '2026-01-01T00:00:00Z');
    INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, created_at, medium_id) VALUES
        ('5b302a80-b1c4-448b-8d1d-2b289e192e6d', 'c4325183-800c-41da-9082-5d998f9a7ecd', 'Track Title One', 1, 1, 1000, '2026-01-01T00:00:00Z', '6ac819de-8e61-419c-9db4-b1582fc9feda'),
        ('85682757-e082-494e-b93f-c5bbb776363d', 'f3dbd598-48d1-44e8-9608-94db537ec89c', 'Track Title Two', 1, 1, 1000, '2026-01-01T00:00:00Z', 'd0fb26fe-9cc5-4116-909d-5376f603c5b7');
";

async fn exec_listed(db: &Database, sql: &'static str) {
    db.call(move |conn| conn.execute_batch(sql).map_err(DbError::from))
        .await
        .unwrap();
}

/// The composer list by name: each composer's id and its work, linked release
/// and unlinked credit counts.
async fn listed_composers(db: &Database) -> Vec<(String, i64, i64, i64)> {
    let sort = [ComposerSortCriterion {
        field: ComposerSortField::Name,
        direction: SortDirection::Ascending,
    }];
    db.get_composer_page(&sort, 0, 50)
        .await
        .unwrap()
        .into_iter()
        .map(|composer| {
            (
                composer.artist.id,
                composer.work_count,
                composer.linked_release_count,
                composer.unlinked_credit_count,
            )
        })
        .collect()
}

#[tokio::test]
async fn composer_list_counts_a_merged_composer_under_its_survivor() {
    let (db, _tmp) = super::temp_db().await;
    exec_listed(&db, LISTED_RELEASES).await;
    exec_listed(
        &db,
        "
        INSERT INTO artists (id, name, name_key, created_at) VALUES
            ('87afe278-5071-4811-a8fb-06ad2ed2d01c', 'Composer Name Survivor', 'composer name survivor', '2026-01-01T00:00:00Z'),
            ('cc0b10bd-7e72-4e7a-85f6-6c6d5d5dc691', 'Composer Name Absorbed', 'composer name absorbed', '2026-01-01T00:00:00Z');
        INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES
            ('65c7e99f-2638-4086-aac4-9dceab1ae6ef', 'Work Title One', 'work', 'mb-work-one', '2026-01-01T00:00:00Z'),
            ('6cfd8091-60c1-4879-ac82-2ab93fd09ca0', 'Work Title Two', 'work', 'mb-work-two', '2026-01-01T00:00:00Z');
        INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES
            ('64ea716b-8c80-4f63-90d5-8d90fbf8fac2', '65c7e99f-2638-4086-aac4-9dceab1ae6ef', '87afe278-5071-4811-a8fb-06ad2ed2d01c', 0, 'musicbrainz', '2026-01-01T00:00:00Z'),
            ('722dc641-3532-46a1-81f8-f415c27e5ef1', '6cfd8091-60c1-4879-ac82-2ab93fd09ca0', 'cc0b10bd-7e72-4e7a-85f6-6c6d5d5dc691', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
        INSERT INTO track_works (id, track_id, work_id, position, source, created_at) VALUES
            ('f0fb93c4-05c4-4ae4-ba02-be9ce5fbdbe5', '5b302a80-b1c4-448b-8d1d-2b289e192e6d', '65c7e99f-2638-4086-aac4-9dceab1ae6ef', 0, 'musicbrainz', '2026-01-01T00:00:00Z'),
            ('182ad394-1b71-44d7-9a48-b0e40d6d05ec', '85682757-e082-494e-b93f-c5bbb776363d', '6cfd8091-60c1-4879-ac82-2ab93fd09ca0', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
        ",
    )
    .await;
    assert_eq!(
        listed_composers(&db).await,
        vec![
            (LISTED_ABSORBED.to_string(), 1, 1, 0),
            (LISTED_SURVIVOR.to_string(), 1, 1, 0),
        ]
    );

    exec_listed(
        &db,
        "INSERT INTO artist_merges (id, into_artist_id, created_at) VALUES
            ('cc0b10bd-7e72-4e7a-85f6-6c6d5d5dc691', '87afe278-5071-4811-a8fb-06ad2ed2d01c', '2026-01-01T00:00:00Z');",
    )
    .await;
    assert_eq!(
        listed_composers(&db).await,
        vec![(LISTED_SURVIVOR.to_string(), 2, 2, 0)],
        "the absorbed composer's work and release count under the survivor"
    );
    let survivor = db
        .find_composer_detail(LISTED_SURVIVOR)
        .await
        .unwrap()
        .expect("the survivor is a composer");
    assert_eq!(survivor.work_groups.len(), 2);
    assert!(db
        .find_composer_detail(LISTED_ABSORBED)
        .await
        .unwrap()
        .is_none());

    exec_listed(
        &db,
        "DELETE FROM artist_merges WHERE id = 'cc0b10bd-7e72-4e7a-85f6-6c6d5d5dc691';",
    )
    .await;
    assert_eq!(
        listed_composers(&db).await,
        vec![
            (LISTED_ABSORBED.to_string(), 1, 1, 0),
            (LISTED_SURVIVOR.to_string(), 1, 1, 0),
        ]
    );
}

#[tokio::test]
async fn composer_list_counts_a_role_credit_until_a_work_links_it() {
    let (db, _tmp) = super::temp_db().await;
    exec_listed(&db, LISTED_RELEASES).await;
    exec_listed(
        &db,
        "
        INSERT INTO artists (id, name, name_key, created_at) VALUES
            ('60b490bc-23a7-41c3-8d2f-fe49880c33a8', 'Composer Name Credited', 'composer name credited', '2026-01-01T00:00:00Z');
        INSERT INTO track_artist_roles (id, track_id, artist_id, position, source, source_credit, created_at) VALUES
            ('d413a2a9-7cc4-4d1c-a0bb-da462bb79f03', '5b302a80-b1c4-448b-8d1d-2b289e192e6d', '60b490bc-23a7-41c3-8d2f-fe49880c33a8', 0, 'musicbrainz', 'composer', '2026-01-01T00:00:00Z');
        INSERT INTO release_artist_roles (id, release_id, artist_id, position, source, source_credit, created_at) VALUES
            ('1f1d760c-41ce-4db7-9c9a-7145f87d1864', 'c4325183-800c-41da-9082-5d998f9a7ecd', '60b490bc-23a7-41c3-8d2f-fe49880c33a8', 0, 'discogs', 'Written-By', '2026-01-01T00:00:00Z');
        ",
    )
    .await;
    assert_eq!(
        listed_composers(&db).await,
        vec![(LISTED_CREDITED.to_string(), 0, 0, 2)]
    );
    let credited = db
        .find_composer_detail(LISTED_CREDITED)
        .await
        .unwrap()
        .expect("an artist with unlinked credits is a composer");
    assert_eq!(credited.unlinked_track_roles.len(), 1);
    assert_eq!(credited.unlinked_release_roles.len(), 1);

    // The track performs a work the artist wrote: both credits are linked.
    exec_listed(
        &db,
        "
        INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES
            ('65c7e99f-2638-4086-aac4-9dceab1ae6ef', 'Work Title One', 'work', 'mb-work-one', '2026-01-01T00:00:00Z');
        INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES
            ('23a58b6e-9ceb-4680-89bd-9cb94ce23168', '65c7e99f-2638-4086-aac4-9dceab1ae6ef', '60b490bc-23a7-41c3-8d2f-fe49880c33a8', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
        INSERT INTO track_works (id, track_id, work_id, position, source, created_at) VALUES
            ('50cfdf21-4727-4c40-b3e4-43520a570be8', '5b302a80-b1c4-448b-8d1d-2b289e192e6d', '65c7e99f-2638-4086-aac4-9dceab1ae6ef', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
        ",
    )
    .await;
    assert_eq!(
        listed_composers(&db).await,
        vec![(LISTED_CREDITED.to_string(), 1, 1, 0)]
    );
    let credited = db
        .find_composer_detail(LISTED_CREDITED)
        .await
        .unwrap()
        .expect("the work's composer");
    assert!(credited.unlinked_track_roles.is_empty());
    assert!(credited.unlinked_release_roles.is_empty());

    // Deleting the release takes its track, credits and links with it; the
    // work stays.
    exec_listed(
        &db,
        "DELETE FROM releases WHERE id = 'c4325183-800c-41da-9082-5d998f9a7ecd';",
    )
    .await;
    assert_eq!(
        listed_composers(&db).await,
        vec![(LISTED_CREDITED.to_string(), 1, 0, 0)]
    );

    exec_listed(
        &db,
        "DELETE FROM works WHERE id = '65c7e99f-2638-4086-aac4-9dceab1ae6ef';",
    )
    .await;
    assert!(listed_composers(&db).await.is_empty());
    assert!(db
        .find_composer_detail(LISTED_CREDITED)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn composer_page_sorts_names_descending_with_ids_ascending() {
    let (db, _tmp) = super::temp_db().await;
    exec_listed(
        &db,
        "
        INSERT INTO artists (id, name, name_key, created_at) VALUES
            ('7aba84fe-423d-44f7-94b4-86bb73d0f49c', 'Composer Name Shared', 'composer name shared', '2026-01-01T00:00:00Z'),
            ('4a571906-c332-438d-be58-8000905905cc', 'Composer Name Shared', 'composer name shared', '2026-01-01T00:00:00Z'),
            ('d2b68adb-3657-460b-9d3b-95457125c403', 'Composer Name Later', 'composer name later', '2026-01-01T00:00:00Z');
        INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES
            ('a354b0c2-26c8-4b22-8226-0b9eaebc31be', 'Work Title A', 'work', 'mb-work-a', '2026-01-01T00:00:00Z');
        INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES
            ('f84bd6f2-0e9c-4497-a1e0-e045716c9950', 'a354b0c2-26c8-4b22-8226-0b9eaebc31be', '7aba84fe-423d-44f7-94b4-86bb73d0f49c', 0, 'file_tags', '2026-01-01T00:00:00Z'),
            ('cdbeca53-3fec-454f-b593-9a4d7e7e0bbb', 'a354b0c2-26c8-4b22-8226-0b9eaebc31be', '4a571906-c332-438d-be58-8000905905cc', 1, 'file_tags', '2026-01-01T00:00:00Z'),
            ('b60f94b2-1f50-4151-92dd-a3632e755e75', 'a354b0c2-26c8-4b22-8226-0b9eaebc31be', 'd2b68adb-3657-460b-9d3b-95457125c403', 2, 'file_tags', '2026-01-01T00:00:00Z');
        ",
    )
    .await;
    let sort = [ComposerSortCriterion {
        field: ComposerSortField::Name,
        direction: SortDirection::Descending,
    }];
    let ids = db
        .get_composer_page(&sort, 0, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|composer| composer.artist.id)
        .collect::<Vec<_>>();
    // "Shared" sorts after "Later"; the two sharing it go by id, ascending.
    assert_eq!(
        ids,
        vec![
            "4a571906-c332-438d-be58-8000905905cc",
            "7aba84fe-423d-44f7-94b4-86bb73d0f49c",
            "d2b68adb-3657-460b-9d3b-95457125c403",
        ]
    );
}
