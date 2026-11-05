//! The composer views: the composer list, a composer's detail, library
//! search over composers and works, and the list's live query.

use crate::*;
use bae_records::test_fixtures::aggregate_ordering::*;
use bae_records::test_fixtures::composed_work::*;
use bae_records::test_fixtures::live_query::*;
use bae_records::test_fixtures::*;
use bae_store::test_fixtures::live_query::*;
use bae_store::test_fixtures::*;
use bae_store::test_fixtures::{exec, exec_batch};
use bae_util::test_channels::HANG_GUARD;
use std::collections::BTreeSet;

const COMPOSER_A: &str = "5dcc4999-03bd-42cc-8d14-8bf0a05effa3";
const COMPOSER_B: &str = "2b748d47-e5b7-4c40-8716-1e608b9dfc3d";
const COMPOSER_C: &str = "80cd3a5e-7fb7-4766-8ec3-d8e86575743b";
const COMPOSER_SOLO: &str = "4d93d615-4549-45d9-81d9-644f079d59bf";

#[tokio::test]
async fn composer_page_lists_a_part_under_its_parent_with_its_representative_release() {
    let (db, _tmp) = composed_work_db(crate::TEST_SCHEMA).await;

    let rows = crate::LibraryViews::new(db.clone())
        .get_composer_detail_rows(ARTIST_COMPOSER, 0, 10)
        .await
        .unwrap();

    let works = rows
        .into_iter()
        .map(|row| match row.item {
            DbComposerDetailItem::Work { work, part } => {
                (work.work.id, part, work.representative_release_id)
            }
            other => panic!("the composer holds no credits: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        works,
        vec![
            (WORK_PARENT_A.to_string(), false, None),
            (WORK_CHILD_A.to_string(), true, Some(RELEASE_A.to_string())),
        ]
    );
}

#[tokio::test]
async fn work_detail_lists_child_works_with_their_representative_release() {
    let (db, _tmp) = composed_work_db(crate::TEST_SCHEMA).await;

    let detail = crate::LibraryViews::new(db.clone())
        .find_work_detail(WORK_PARENT_A)
        .await
        .unwrap()
        .expect("work detail");

    assert_eq!(detail.child_works.len(), 1);
    assert_eq!(detail.child_works[0].work.id, WORK_CHILD_A);
    assert_eq!(detail.child_works[0].representative_release_id.as_deref(), Some(RELEASE_A));
}

#[tokio::test]
async fn work_detail_release_rows_carry_album_release_display_fields() {
    let (db, _tmp) = composed_work_db(crate::TEST_SCHEMA).await;

    let detail = crate::LibraryViews::new(db.clone())
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
    assert_eq!(
        release.media,
        bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 1).media
    );
    assert_eq!(release.release_index, 1);
}

#[tokio::test]
async fn composer_page_uses_id_tiebreaker() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, name_key, sort_name, discogs_artist_id, musicbrainz_artist_id, created_at) VALUES ('80cd3a5e-7fb7-4766-8ec3-d8e86575743b', 'Composer Name Shared', 'composer name shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z'),
             ('5dcc4999-03bd-42cc-8d14-8bf0a05effa3', 'Composer Name Shared', 'composer name shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z'),
             ('2b748d47-e5b7-4c40-8716-1e608b9dfc3d', 'Composer Name Shared', 'composer name shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z');

            INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES ('432c8996-8af0-43dc-868a-822a256f65c4', 'Work Title A', 'work', 'mb-work-a', '2026-01-01T00:00:00Z'),
             ('d866f97d-e57f-45e8-8c4e-f81ad8717882', 'Work Title B', 'work', 'mb-work-b', '2026-01-01T00:00:00Z'),
             ('00e1ff99-c327-477d-846d-28d2f27fa004', 'Work Title C', 'work', 'mb-work-c', '2026-01-01T00:00:00Z');

            INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES ('f6fc3bd0-99db-4e6e-8efa-c8e5a1c35d74', '00e1ff99-c327-477d-846d-28d2f27fa004', '80cd3a5e-7fb7-4766-8ec3-d8e86575743b', 0, 'file_tags', '2026-01-01T00:00:00Z'),
             ('ec41a8cd-a9a4-473e-8b70-d78168aefd8e', '432c8996-8af0-43dc-868a-822a256f65c4', '5dcc4999-03bd-42cc-8d14-8bf0a05effa3', 0, 'file_tags', '2026-01-01T00:00:00Z'),
             ('08d921ba-8e6b-4b5b-8583-498117bbefe4', 'd866f97d-e57f-45e8-8c4e-f81ad8717882', '2b748d47-e5b7-4c40-8716-1e608b9dfc3d', 0, 'file_tags', '2026-01-01T00:00:00Z');
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
        let page =
            crate::LibraryViews::new(db.clone()).get_composer_page(&sort, offset, 1).await.unwrap();
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
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, name_key, sort_name, discogs_artist_id, musicbrainz_artist_id, created_at) VALUES ('5dcc4999-03bd-42cc-8d14-8bf0a05effa3', 'Composer Name A', 'composer name a', NULL, NULL, NULL, '2026-01-01T00:00:00Z'),
             ('2b748d47-e5b7-4c40-8716-1e608b9dfc3d', 'Composer Name B', 'composer name b', NULL, NULL, NULL, '2026-01-01T00:00:00Z'),
             ('4d93d615-4549-45d9-81d9-644f079d59bf', 'Composer Name Solo', 'composer name solo', NULL, NULL, NULL, '2026-01-01T00:00:00Z');

            INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES ('1d446150-576e-479f-87a7-40ac7a511fa1', 'Work Title A1', 'work', 'mb-work-a1', '2026-01-01T00:00:00Z'),
             ('6a32dca0-bf5b-4baa-829d-dc2ef531e763', 'Work Title A2', 'work', 'mb-work-a2', '2026-01-01T00:00:00Z'),
             ('735e697c-f2ce-4512-806c-4f872446f6e6', 'Work Title B1', 'work', 'mb-work-b1', '2026-01-01T00:00:00Z'),
             ('d5f30cc0-a35a-4294-851b-ce2d9c172d1c', 'Work Title B2', 'work', 'mb-work-b2', '2026-01-01T00:00:00Z'),
             ('5dc2446b-5241-46e0-8be4-4325e06f1417', 'Work Title Solo', 'work', 'mb-work-solo', '2026-01-01T00:00:00Z');

            INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES ('c6ad84f8-3ff6-4528-8b5e-f90d24c33908', '1d446150-576e-479f-87a7-40ac7a511fa1', '5dcc4999-03bd-42cc-8d14-8bf0a05effa3', 0, 'file_tags', '2026-01-01T00:00:00Z'),
             ('b6b51719-ad5f-4e9e-8aae-0b51883677ce', '6a32dca0-bf5b-4baa-829d-dc2ef531e763', '5dcc4999-03bd-42cc-8d14-8bf0a05effa3', 0, 'file_tags', '2026-01-01T00:00:00Z'),
             ('72f0c48b-5fcf-40ff-8fff-3dc15a0cf06d', '735e697c-f2ce-4512-806c-4f872446f6e6', '2b748d47-e5b7-4c40-8716-1e608b9dfc3d', 0, 'file_tags', '2026-01-01T00:00:00Z'),
             ('7b13d5f1-3836-422f-831a-10ac1435a502', 'd5f30cc0-a35a-4294-851b-ce2d9c172d1c', '2b748d47-e5b7-4c40-8716-1e608b9dfc3d', 0, 'file_tags', '2026-01-01T00:00:00Z'),
             ('aeccde70-29b8-4f81-85c4-5135f2c7a40c', '5dc2446b-5241-46e0-8be4-4325e06f1417', '4d93d615-4549-45d9-81d9-644f079d59bf', 0, 'file_tags', '2026-01-01T00:00:00Z');
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
    let page = crate::LibraryViews::new(db.clone()).get_composer_page(&sort, 0, 10).await.unwrap();
    let ids: Vec<&str> = page.iter().map(|c| c.artist.id.as_str()).collect();

    // composer-a and composer-b tie on work_count (2 each); the secondary
    // Name DESC criterion orders composer-b before composer-a.
    assert_eq!(ids, vec![COMPOSER_B, COMPOSER_A, COMPOSER_SOLO]);
}

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

async fn exec_listed(db: &Store, sql: &'static str) {
    db.call(move |conn| conn.execute_batch(sql).map_err(DbError::from)).await.unwrap();
}

/// The composer list by name: each composer's id and its work, linked release
/// and unlinked credit counts.
async fn listed_composers(db: &Store) -> Vec<(String, i64, i64, i64)> {
    let sort = [ComposerSortCriterion {
        field: ComposerSortField::Name,
        direction: SortDirection::Ascending,
    }];
    crate::LibraryViews::new(db.clone())
        .get_composer_page(&sort, 0, 50)
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
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
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
        vec![(LISTED_ABSORBED.to_string(), 1, 1, 0), (LISTED_SURVIVOR.to_string(), 1, 1, 0),]
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
    let survivor = crate::LibraryViews::new(db.clone())
        .find_composer_detail(LISTED_SURVIVOR)
        .await
        .unwrap()
        .expect("the survivor is a composer");
    assert_eq!(survivor.default_work_id.as_deref(), Some("65c7e99f-2638-4086-aac4-9dceab1ae6ef"));
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_composer_detail_rows(LISTED_SURVIVOR, 0, 10)
            .await
            .unwrap()
            .len(),
        2,
        "the absorbed composer's work is on the survivor's page"
    );
    assert!(crate::LibraryViews::new(db.clone())
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
        vec![(LISTED_ABSORBED.to_string(), 1, 1, 0), (LISTED_SURVIVOR.to_string(), 1, 1, 0),]
    );
}

/// Each composer listing row: its artist and its work, linked release and
/// unlinked credit counts.
fn composer_listing_rows(conn: &rusqlite::Connection) -> Vec<(String, i64, i64, i64)> {
    conn.prepare(
        "SELECT artist_id, work_count, linked_release_count, unlinked_credit_count
         FROM composer_listing ORDER BY artist_id",
    )
    .unwrap()
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

#[test]
fn a_composer_merged_before_it_arrived_is_listed_with_what_was_counted_under_it() {
    // Rows from another device can arrive before the rows they name: here a
    // merge and a work credit before either artist.
    let conn = bae_store::schema::bare_library_connection(crate::TEST_SCHEMA);
    conn.execute_batch(
        "PRAGMA foreign_keys = OFF;
         INSERT INTO artist_merges (id, into_artist_id, created_at) VALUES
            ('cc0b10bd-7e72-4e7a-85f6-6c6d5d5dc691', '87afe278-5071-4811-a8fb-06ad2ed2d01c', '2026-01-01T00:00:00Z');
         INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES
            ('6cfd8091-60c1-4879-ac82-2ab93fd09ca0', 'Work Title Two', 'work', 'mb-work-two', '2026-01-01T00:00:00Z');
         INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES
            ('722dc641-3532-46a1-81f8-f415c27e5ef1', '6cfd8091-60c1-4879-ac82-2ab93fd09ca0', 'cc0b10bd-7e72-4e7a-85f6-6c6d5d5dc691', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
         INSERT INTO artists (id, name, name_key, created_at) VALUES
            ('cc0b10bd-7e72-4e7a-85f6-6c6d5d5dc691', 'Composer Name Absorbed', 'composer name absorbed', '2026-01-01T00:00:00Z');",
    )
    .unwrap();
    assert_eq!(
        composer_listing_rows(&conn),
        vec![],
        "the absorbed artist's work counts under its survivor, which has not arrived"
    );

    conn.execute_batch(
        "INSERT INTO artists (id, name, name_key, created_at) VALUES
            ('87afe278-5071-4811-a8fb-06ad2ed2d01c', 'Composer Name Survivor', 'composer name survivor', '2026-01-01T00:00:00Z');",
    )
    .unwrap();
    assert_eq!(composer_listing_rows(&conn), vec![(LISTED_SURVIVOR.to_string(), 1, 0, 0)]);
}

#[tokio::test]
async fn composer_list_counts_a_role_credit_until_a_work_links_it() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
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
    assert_eq!(listed_composers(&db).await, vec![(LISTED_CREDITED.to_string(), 0, 0, 2)]);
    let credited = crate::LibraryViews::new(db.clone())
        .find_composer_detail(LISTED_CREDITED)
        .await
        .unwrap()
        .expect("an artist with unlinked credits is a composer");
    assert_eq!(credited.default_work_id, None);
    let credits = crate::LibraryViews::new(db.clone())
        .get_composer_detail_rows(LISTED_CREDITED, 0, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|row| match row.item {
            DbComposerDetailItem::ReleaseCredit(credit) => ("release", credit.role.id),
            DbComposerDetailItem::TrackCredit(credit) => ("track", credit.role.id),
            DbComposerDetailItem::Work { .. } => panic!("the artist is credited on no work"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        credits,
        vec![
            ("release", "1f1d760c-41ce-4db7-9c9a-7145f87d1864".to_string()),
            ("track", "d413a2a9-7cc4-4d1c-a0bb-da462bb79f03".to_string()),
        ]
    );

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
    assert_eq!(listed_composers(&db).await, vec![(LISTED_CREDITED.to_string(), 1, 1, 0)]);
    crate::LibraryViews::new(db.clone())
        .find_composer_detail(LISTED_CREDITED)
        .await
        .unwrap()
        .expect("the work's composer");
    let rows = crate::LibraryViews::new(db.clone())
        .get_composer_detail_rows(LISTED_CREDITED, 0, 10)
        .await
        .unwrap();
    assert!(
        rows.iter().all(|row| matches!(row.item, DbComposerDetailItem::Work { .. })),
        "linked credits leave the page: {rows:?}"
    );

    // Deleting the release takes its track, credits and links with it; the
    // work stays.
    exec_listed(&db, "DELETE FROM releases WHERE id = 'c4325183-800c-41da-9082-5d998f9a7ecd';")
        .await;
    assert_eq!(listed_composers(&db).await, vec![(LISTED_CREDITED.to_string(), 1, 0, 0)]);

    exec_listed(&db, "DELETE FROM works WHERE id = '65c7e99f-2638-4086-aac4-9dceab1ae6ef';").await;
    assert!(listed_composers(&db).await.is_empty());
    assert!(crate::LibraryViews::new(db.clone())
        .find_composer_detail(LISTED_CREDITED)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn composer_page_sorts_names_descending_with_ids_ascending() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
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
    let ids = crate::LibraryViews::new(db.clone())
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

/// [`next_album_page_is_only_a_rename`] for the composer grid: rename the one
/// composer `shown` holds.
async fn next_composer_page_is_only_a_rename(
    db: &Store,
    live: &mut coven::ReconfigurableLiveQuery<
        bae_store::LibraryPageWindows,
        ComposerBrowseProjection,
    >,
    shown: ComposerBrowseProjection,
    name: &str,
) -> ComposerBrowseProjection {
    let composer_id = shown.windows[0].rows[0].artist.id.clone();
    exec(db, "UPDATE artists SET name = ?1 WHERE id = ?2", &[name, composer_id.as_str()]).await;
    let renamed = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("renaming the shown composer wakes composer browse")
        .expect("the query stays open")
        .result
        .unwrap();
    let mut expected = shown;
    expected.windows[0].rows[0].artist.name = name.to_string();
    assert_eq!(renamed, expected, "the page carries more than the rename");
    renamed
}

#[tokio::test]
async fn composer_browse_subscription_reconfigures_bounded_windows() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    exec_batch(
        &db,
        &format!(
            "INSERT INTO artists (id, name, name_key, created_at) VALUES ('{COMPOSER_ID}', 'Composer Name First', 'composer name first', '2026-01-01T00:00:00Z'),
             ('{OTHER_COMPOSER_ID}', 'Composer Name Second', 'composer name second', '2026-01-02T00:00:00Z');
             INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES ('{COMPOSER_WORK_ID}', 'Work Title First', 'work', 'work-first', '2026-01-01T00:00:00Z'),
             ('{OTHER_COMPOSER_WORK_ID}', 'Work Title Second', 'work', 'work-second', '2026-01-02T00:00:00Z');
             INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES ('{COMPOSER_LINK_ID}', '{COMPOSER_WORK_ID}', '{COMPOSER_ID}', 0, 'file_tags', '2026-01-01T00:00:00Z'),
             ('{OTHER_COMPOSER_LINK_ID}', '{OTHER_COMPOSER_WORK_ID}', '{OTHER_COMPOSER_ID}', 0, 'file_tags', '2026-01-02T00:00:00Z');"
        ),
    )
    .await;
    let sort = [ComposerSortCriterion {
        field: ComposerSortField::Name,
        direction: SortDirection::Descending,
    }];
    let second_window = LibraryPageWindow { offset: 1, limit: 1 };
    let initial_request = [second_window.clone()].into_iter().collect::<BTreeSet<_>>();
    let mut live = crate::LibraryViews::new(db.clone())
        .subscribe_composer_browse(&sort, initial_request.clone());
    let requests = live.requests();
    let initial_event = live.next().await.expect("the query stays open");
    assert_eq!(initial_event.request, initial_request);
    assert_eq!(initial_event.revision.0, 0);
    let initial = initial_event.result.unwrap();
    assert_eq!(initial.total_count, 2);
    assert_eq!(initial.windows[0].rows[0].artist.id, COMPOSER_ID);

    let both_pages = [LibraryPageWindow { offset: 0, limit: 1 }, second_window.clone()]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let both_pages_revision = requests.set(both_pages.clone()).unwrap();
    let both_event = live.next().await.expect("the query stays open");
    assert_eq!(both_event.revision, both_pages_revision);
    let both = both_event.result.unwrap();
    assert_eq!(both.windows.len(), 2);
    assert_eq!(both.windows[0].rows[0].artist.id, OTHER_COMPOSER_ID);
    assert_eq!(both.windows[1].rows[0].artist.id, COMPOSER_ID);

    exec(
        &db,
        "UPDATE artists SET name = 'Composer Name Renamed', name_key = 'composer name renamed' WHERE id = ?1",
        &[OTHER_COMPOSER_ID],
    )
    .await;
    let renamed_event = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("non-first composer metadata wakes composer browse")
        .expect("the query stays open");
    assert_eq!(renamed_event.revision, both_pages_revision);
    let renamed = renamed_event.result.unwrap();
    assert_eq!(
        renamed.windows.iter().map(|window| window.rows[0].artist.id.as_str()).collect::<Vec<_>>(),
        vec![OTHER_COMPOSER_ID, COMPOSER_ID]
    );
    assert_eq!(renamed.windows[0].rows[0].artist.name, "Composer Name Renamed");

    requests.set([second_window].into_iter().collect::<BTreeSet<_>>()).unwrap();
    let hidden_other_composer = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(hidden_other_composer.windows[0].rows[0].artist.id, COMPOSER_ID);

    // The edited composer is outside the requested window, so its metadata
    // changes nothing the browse delivers.
    exec(
        &db,
        "UPDATE artists SET sort_name = 'Composer Sort Hidden' WHERE id = ?1",
        &[OTHER_COMPOSER_ID],
    )
    .await;
    let shown = next_composer_page_is_only_a_rename(
        &db,
        &mut live,
        hidden_other_composer,
        "Composer Name First Shown",
    )
    .await;

    let composer_image =
        write_artist_image(&records, OTHER_COMPOSER_ID, "composer image bytes").await;
    // Only the requested rows' images are read, so an image on the composer
    // outside the window changes nothing the browse delivers.
    next_composer_page_is_only_a_rename(&db, &mut live, shown, "Composer Name First Again").await;
    let other_composer_page =
        [LibraryPageWindow { offset: 0, limit: 1 }].into_iter().collect::<BTreeSet<_>>();
    requests.set(other_composer_page).unwrap();
    let imaged = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(imaged.windows[0].rows[0].artist.id, OTHER_COMPOSER_ID);
    assert_eq!(
        imaged.windows[0].rows[0].image.as_ref().map(|image| image.version.as_str()),
        Some(composer_image.as_str())
    );

    let replaced_composer_image =
        write_artist_image(&records, OTHER_COMPOSER_ID, "replaced composer image bytes").await;
    let image_updated = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a requested composer's image version wakes composer browse")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(
        image_updated.windows[0].rows[0].image.as_ref().map(|image| image.version.as_str()),
        Some(replaced_composer_image.as_str())
    );

    requests.set(BTreeSet::new()).unwrap();
    let count_only = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(count_only.total_count, 2);
    assert!(count_only.windows.is_empty());

    exec_batch(
        &db,
        &format!(
            "INSERT INTO artists (id, name, name_key, created_at) VALUES ('{INSERTED_COMPOSER_ID}', 'Composer Name Inserted', 'composer name inserted', '2026-01-03T00:00:00Z');
             INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES ('{INSERTED_COMPOSER_WORK_ID}', 'Work Title Inserted', 'work', 'work-inserted', '2026-01-03T00:00:00Z');
             INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES ('{INSERTED_COMPOSER_LINK_ID}', '{INSERTED_COMPOSER_WORK_ID}', '{INSERTED_COMPOSER_ID}', 0, 'file_tags', '2026-01-03T00:00:00Z');"
        ),
    )
    .await;
    let inserted = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(inserted.total_count, 3);
    assert!(inserted.windows.is_empty());

    exec(&db, "DELETE FROM work_artists WHERE id = ?1", &[INSERTED_COMPOSER_LINK_ID]).await;
    let deleted = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(deleted.total_count, 2);
    assert!(deleted.windows.is_empty());
}

/// The composer list on B is a local table B's triggers keep from the synced
/// rows: applying A's writes runs them, and B's live query delivers what they
/// wrote. A's work credit lists its composer; A's merge then lists the work
/// under the survivor alone.
#[test]
fn composer_browse_subscription_delivers_listings_kept_by_applied_writes() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let writer = devices.a();
        let reader = devices.b();
        let sort = [ComposerSortCriterion {
            field: ComposerSortField::Name,
            direction: SortDirection::Ascending,
        }];
        let mut live =
            crate::LibraryViews::new(reader.clone()).subscribe_composer_browse(&sort, first_page());
        assert_eq!(live.next().await.expect("the query stays open").result.unwrap().total_count, 0);
        let listed = async |live: &mut coven::ReconfigurableLiveQuery<
            bae_store::LibraryPageWindows,
            ComposerBrowseProjection,
        >,
                            expected: Vec<(&str, i64)>| {
            tokio::time::timeout(HANG_GUARD, async {
                loop {
                    let shown = live.next().await.expect("the query stays open").result.unwrap();
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
            .settle("the remote work credits", async |_, db| {
                crate::LibraryViews::new(db.clone())
                    .find_composer_detail(OTHER_COMPOSER_ID)
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
            .settle("the remote merge", async |_, db| {
                crate::LibraryViews::new(db.clone())
                    .find_composer_detail(OTHER_COMPOSER_ID)
                    .await
                    .unwrap()
                    .is_none()
            })
            .await;
        let merged = listed(&mut live, vec![(COMPOSER_ID, 2)]).await;
        assert_eq!(merged.total_count, 1);
    });
}

#[tokio::test]
async fn work_summary_orders_composer_names_inside_the_aggregate() {
    let (db, _tmp) = aggregate_db(crate::TEST_SCHEMA).await;
    let results =
        crate::LibraryViews::new(db.clone()).search_rows("Work Title A", 10).await.unwrap();
    let summary = results.works.first().expect("work summary row");

    assert_eq!(
        summary.composer_names.as_deref(),
        Some("Composer Name First, Composer Name Second")
    );
}

/// A work performed on releases of two albums, for the cover tests: `Album
/// Beta` performs it on its only release's first track; `album alpha`, first
/// by title compared without case, holds a primary release that doesn't
/// perform it, then two that do, the later of them on its first track.
async fn work_across_albums_db() -> (Store, tempfile::TempDir) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    exec_batch(
        &db,
        "
        INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES
            ('11111111-0000-4000-8000-000000000001', 'Album Beta', 2026, 0, '2026-01-01T00:00:00Z'),
            ('11111111-0000-4000-8000-000000000002', 'album alpha', 2026, 0, '2026-01-01T00:00:00Z');

        INSERT INTO releases (id, album_id, release_name, year, discogs_details, source_folder_name, album_loudness_lufs, album_peak_linear, created_at, album_order) VALUES
            ('22222222-0000-4000-8000-000000000001', '11111111-0000-4000-8000-000000000001', NULL, 2026, '[]', NULL, NULL, NULL, '2026-01-01T00:00:00Z', 0),
            ('22222222-0000-4000-8000-000000000002', '11111111-0000-4000-8000-000000000002', NULL, 2026, '[]', NULL, NULL, NULL, '2026-01-01T00:00:00Z', 0),
            ('22222222-0000-4000-8000-000000000004', '11111111-0000-4000-8000-000000000002', NULL, 2026, '[]', NULL, NULL, NULL, '2026-01-01T00:00:00Z', 1),
            ('22222222-0000-4000-8000-000000000003', '11111111-0000-4000-8000-000000000002', NULL, 2026, '[]', NULL, NULL, NULL, '2026-01-01T00:00:00Z', 2);

        INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) VALUES
            ('33333333-0000-4000-8000-000000000001', '22222222-0000-4000-8000-000000000001', 1, 'cd', 1, '2026-01-01T00:00:00Z'),
            ('33333333-0000-4000-8000-000000000002', '22222222-0000-4000-8000-000000000002', 1, 'cd', 1, '2026-01-01T00:00:00Z'),
            ('33333333-0000-4000-8000-000000000004', '22222222-0000-4000-8000-000000000004', 1, 'cd', 1, '2026-01-01T00:00:00Z'),
            ('33333333-0000-4000-8000-000000000003', '22222222-0000-4000-8000-000000000003', 1, 'cd', 1, '2026-01-01T00:00:00Z');

        INSERT INTO tracks (id, release_id, title, side, track_number, position, duration_ms, created_at, medium_id) VALUES
            ('44444444-0000-4000-8000-000000000001', '22222222-0000-4000-8000-000000000001', 'Beta Track', 1, 1, 0, 1000, '2026-01-01T00:00:00Z', '33333333-0000-4000-8000-000000000001'),
            ('44444444-0000-4000-8000-000000000002', '22222222-0000-4000-8000-000000000002', 'Alpha Primary Track', 1, 1, 0, 1000, '2026-01-01T00:00:00Z', '33333333-0000-4000-8000-000000000002'),
            ('44444444-0000-4000-8000-000000000004', '22222222-0000-4000-8000-000000000004', 'Alpha Second Track', 1, 5, 4, 1000, '2026-01-01T00:00:00Z', '33333333-0000-4000-8000-000000000004'),
            ('44444444-0000-4000-8000-000000000003', '22222222-0000-4000-8000-000000000003', 'Alpha Third Track', 1, 1, 0, 1000, '2026-01-01T00:00:00Z', '33333333-0000-4000-8000-000000000003');

        INSERT INTO works (id, title, disambiguation, work_type, musicbrainz_work_id, created_at) VALUES
            ('55555555-0000-4000-8000-000000000001', 'Work Across Albums', NULL, 'work', 'mb-work-across-albums', '2026-01-01T00:00:00Z');

        INSERT INTO track_works (id, track_id, work_id, position, source, created_at) VALUES
            ('66666666-0000-4000-8000-000000000001', '44444444-0000-4000-8000-000000000001', '55555555-0000-4000-8000-000000000001', 0, 'musicbrainz', '2026-01-01T00:00:00Z'),
            ('66666666-0000-4000-8000-000000000004', '44444444-0000-4000-8000-000000000004', '55555555-0000-4000-8000-000000000001', 0, 'musicbrainz', '2026-01-01T00:00:00Z'),
            ('66666666-0000-4000-8000-000000000003', '44444444-0000-4000-8000-000000000003', '55555555-0000-4000-8000-000000000001', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
        ",
    )
    .await;
    (db, tmp)
}

const WORK_ACROSS_ALBUMS: &str = "55555555-0000-4000-8000-000000000001";
const RELEASE_ALPHA_PRIMARY: &str = "22222222-0000-4000-8000-000000000002";
const RELEASE_ALPHA_SECOND: &str = "22222222-0000-4000-8000-000000000004";

async fn representative_release(db: &Store) -> Option<String> {
    crate::LibraryViews::new(db.clone())
        .find_work_detail(WORK_ACROSS_ALBUMS)
        .await
        .unwrap()
        .expect("the work is in the library")
        .work
        .representative_release_id
}

/// A work's cover comes from its first album by title, compared without case
/// as the page compares titles, not from the track that performs it first:
/// that album's primary release performs none of it, so the cover is its
/// first release in album order that does.
#[tokio::test]
async fn a_works_cover_is_its_first_albums_first_release_performing_it() {
    let (db, _tmp) = work_across_albums_db().await;
    assert_eq!(representative_release(&db).await.as_deref(), Some(RELEASE_ALPHA_SECOND));
}

/// When the first album's primary release performs the work, its cover is the
/// work's, though a later release performs the work on an earlier track.
#[tokio::test]
async fn a_works_cover_is_its_first_albums_primary_release_when_it_performs_it() {
    let (db, _tmp) = work_across_albums_db().await;
    exec_batch(
        &db,
        "INSERT INTO track_works (id, track_id, work_id, position, source, created_at) VALUES
            ('66666666-0000-4000-8000-000000000002', '44444444-0000-4000-8000-000000000002', '55555555-0000-4000-8000-000000000001', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
         UPDATE tracks SET track_number = 4, position = 3
            WHERE id = '44444444-0000-4000-8000-000000000002';",
    )
    .await;
    assert_eq!(representative_release(&db).await.as_deref(), Some(RELEASE_ALPHA_PRIMARY));
}

/// A work's page lists the releases performing it, and their tracks, by album
/// title compared without case, as the cover rule picks its album: `album
/// alpha`'s releases come before `Album Beta`'s. An album sharing a title with
/// another follows it whole, by id, rather than interleaving its releases with
/// the other's by place.
#[tokio::test]
async fn a_works_page_lists_its_albums_by_title_without_case_each_album_whole() {
    let (db, _tmp) = work_across_albums_db().await;
    exec_batch(
        &db,
        "
        INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES
            ('11111111-0000-4000-8000-000000000003', 'album alpha', 2026, 0, '2026-01-01T00:00:00Z');
        INSERT INTO releases (id, album_id, release_name, year, discogs_details, source_folder_name, album_loudness_lufs, album_peak_linear, created_at, album_order) VALUES
            ('22222222-0000-4000-8000-000000000005', '11111111-0000-4000-8000-000000000003', NULL, 2026, '[]', NULL, NULL, NULL, '2026-01-01T00:00:00Z', 1);
        INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) VALUES
            ('33333333-0000-4000-8000-000000000005', '22222222-0000-4000-8000-000000000005', 1, 'cd', 1, '2026-01-01T00:00:00Z');
        INSERT INTO tracks (id, release_id, title, side, track_number, position, duration_ms, created_at, medium_id) VALUES
            ('44444444-0000-4000-8000-000000000005', '22222222-0000-4000-8000-000000000005', 'Alpha Twin Track', 1, 1, 0, 1000, '2026-01-01T00:00:00Z', '33333333-0000-4000-8000-000000000005');
        INSERT INTO track_works (id, track_id, work_id, position, source, created_at) VALUES
            ('66666666-0000-4000-8000-000000000005', '44444444-0000-4000-8000-000000000005', '55555555-0000-4000-8000-000000000001', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
        ",
    )
    .await;

    let detail = crate::LibraryViews::new(db.clone())
        .find_work_detail(WORK_ACROSS_ALBUMS)
        .await
        .unwrap()
        .expect("the work is in the library");

    let in_order = [
        RELEASE_ALPHA_SECOND,
        "22222222-0000-4000-8000-000000000003",
        "22222222-0000-4000-8000-000000000005",
        "22222222-0000-4000-8000-000000000001",
    ];
    assert_eq!(
        detail.releases.iter().map(|release| release.release_id.as_str()).collect::<Vec<_>>(),
        in_order
    );
    assert_eq!(
        detail.tracks.iter().map(|track| track.track.release_id.as_str()).collect::<Vec<_>>(),
        in_order
    );
}
