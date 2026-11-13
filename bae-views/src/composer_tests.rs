//! The composer views: the composer list, a composer's detail, library
//! search over composers and works, and the list's live query.

use crate::*;
use bae_catalogs::catalog::Catalog;
use bae_records::test_fixtures::aggregate_ordering::*;
use bae_records::test_fixtures::artists_apart::{
    meet_musicbrainz_artist, merge_apart, musicbrainz_artist,
};
use bae_records::test_fixtures::composed_work::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::live_query::*;
use bae_records::test_fixtures::*;
use bae_store::test_fixtures::live_query::*;
use bae_store::test_fixtures::*;
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
    // Three artists sharing a name, told apart by their MusicBrainz ids.
    let mut library = RecordsFixture::new();
    for (composer, work, title) in [
        (COMPOSER_C, "00e1ff99-c327-477d-846d-28d2f27fa004", "Work Title C"),
        (COMPOSER_A, "432c8996-8af0-43dc-868a-822a256f65c4", "Work Title A"),
        (COMPOSER_B, "d866f97d-e57f-45e8-8c4e-f81ad8717882", "Work Title B"),
    ] {
        library = library
            .artist(composer, "Composer Name Shared")
            .catalog_ids(composer, None, Some(&format!("mb-{composer}")))
            .work(work, title)
            .composer(work, composer);
    }
    library.write(&test_records(&db)).await;

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
    RecordsFixture::new()
        .artist(COMPOSER_A, "Composer Name A")
        .artist(COMPOSER_B, "Composer Name B")
        .artist(COMPOSER_SOLO, "Composer Name Solo")
        .work("1d446150-576e-479f-87a7-40ac7a511fa1", "Work Title A1")
        .composer("1d446150-576e-479f-87a7-40ac7a511fa1", COMPOSER_A)
        .work("6a32dca0-bf5b-4baa-829d-dc2ef531e763", "Work Title A2")
        .composer("6a32dca0-bf5b-4baa-829d-dc2ef531e763", COMPOSER_A)
        .work("735e697c-f2ce-4512-806c-4f872446f6e6", "Work Title B1")
        .composer("735e697c-f2ce-4512-806c-4f872446f6e6", COMPOSER_B)
        .work("d5f30cc0-a35a-4294-851b-ce2d9c172d1c", "Work Title B2")
        .composer("d5f30cc0-a35a-4294-851b-ce2d9c172d1c", COMPOSER_B)
        .work("5dc2446b-5241-46e0-8be4-4325e06f1417", "Work Title Solo")
        .composer("5dc2446b-5241-46e0-8be4-4325e06f1417", COMPOSER_SOLO)
        .write(&test_records(&db))
        .await;

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

const LISTED_RELEASE_ONE: &str = "c4325183-800c-41da-9082-5d998f9a7ecd";
const LISTED_RELEASE_TWO: &str = "f3dbd598-48d1-44e8-9608-94db537ec89c";
const LISTED_TRACK_ONE: &str = "5b302a80-b1c4-448b-8d1d-2b289e192e6d";
const LISTED_TRACK_TWO: &str = "85682757-e082-494e-b93f-c5bbb776363d";
const LISTED_WORK_ONE: &str = "65c7e99f-2638-4086-aac4-9dceab1ae6ef";
const LISTED_WORK_TWO: &str = "6cfd8091-60c1-4879-ac82-2ab93fd09ca0";

/// Two releases of one track each, on albums of their own.
fn listed_releases() -> RecordsFixture {
    RecordsFixture::new()
        .album("b8f2ed98-6074-4a3f-87d8-19598807e767", "Album Title One", Some(2026), &[])
        .release("b8f2ed98-6074-4a3f-87d8-19598807e767", LISTED_RELEASE_ONE)
        .tracks(LISTED_RELEASE_ONE, &[(LISTED_TRACK_ONE, "Track Title One")])
        .album("bb2d8f2e-43ec-4e07-a430-0b9950b88d92", "Album Title Two", Some(2026), &[])
        .release("bb2d8f2e-43ec-4e07-a430-0b9950b88d92", LISTED_RELEASE_TWO)
        .tracks(LISTED_RELEASE_TWO, &[(LISTED_TRACK_TWO, "Track Title Two")])
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

/// Two composers each credited on a work, merged when the devices that each
/// gave one of them a Discogs id meet: the survivor, the smaller id, lists
/// both works and both releases.
#[test]
fn composer_list_counts_a_merged_composer_under_its_survivor() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let db = devices.a();
        listed_releases()
            .artist(LISTED_SURVIVOR, "Composer Name Survivor")
            .artist(LISTED_ABSORBED, "Composer Name Absorbed")
            .work(LISTED_WORK_ONE, "Work Title One")
            .composer(LISTED_WORK_ONE, LISTED_SURVIVOR)
            .work(LISTED_WORK_TWO, "Work Title Two")
            .composer(LISTED_WORK_TWO, LISTED_ABSORBED)
            .performs(LISTED_TRACK_ONE, LISTED_WORK_ONE)
            .performs(LISTED_TRACK_TWO, LISTED_WORK_TWO)
            .write(&test_records(db))
            .await;
        assert_eq!(
            listed_composers(db).await,
            vec![(LISTED_ABSORBED.to_string(), 1, 1, 0), (LISTED_SURVIVOR.to_string(), 1, 1, 0),]
        );
        devices.settle("both composers", async |_, db| listed_composers(db).await.len() == 2).await;

        merge_apart(
            devices,
            ("Composer Name Survivor", "Composer Name Absorbed"),
            "discogs-composer",
        )
        .await;
        assert_eq!(
            listed_composers(db).await,
            vec![(LISTED_SURVIVOR.to_string(), 2, 2, 0)],
            "the absorbed composer's work and release count under the survivor"
        );
        let survivor = crate::LibraryViews::new(db.clone())
            .find_composer_detail(LISTED_SURVIVOR)
            .await
            .unwrap()
            .expect("the survivor is a composer");
        assert_eq!(survivor.default_work_id.as_deref(), Some(LISTED_WORK_ONE));
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
    });
}

#[tokio::test]
async fn composer_list_counts_a_role_credit_until_a_work_links_it() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let records = test_records(&db);
    let track_role = "d413a2a9-7cc4-4d1c-a0bb-da462bb79f03";
    let release_role = "1f1d760c-41ce-4db7-9c9a-7145f87d1864";
    let role_credits = || {
        RecordsFixture::new()
            .track_role(
                track_role,
                LISTED_TRACK_ONE,
                LISTED_CREDITED,
                (Catalog::MusicBrainz, "composer"),
            )
            .release_role(
                release_role,
                LISTED_RELEASE_ONE,
                LISTED_CREDITED,
                (Catalog::Discogs, "Written-By"),
            )
    };
    listed_releases()
        .artist(LISTED_CREDITED, "Composer Name Credited")
        .edit(|rows| {
            let roles = role_credits().rows();
            rows.track_artist_roles = roles.track_artist_roles;
            rows.release_artist_roles = roles.release_artist_roles;
        })
        .write(&records)
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
        vec![("release", release_role.to_string()), ("track", track_role.to_string()),]
    );

    // An edit applying a catalog finds the track performing a work the artist
    // wrote: both credits are linked.
    records
        .apply_release_links_for_test(
            LISTED_RELEASE_ONE,
            role_credits()
                .work(LISTED_WORK_ONE, "Work Title One")
                .composer(LISTED_WORK_ONE, LISTED_CREDITED)
                .performs(LISTED_TRACK_ONE, LISTED_WORK_ONE)
                .rows(),
        )
        .await
        .unwrap();
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
    delete_release(&records, LISTED_RELEASE_ONE).await;
    assert_eq!(listed_composers(&db).await, vec![(LISTED_CREDITED.to_string(), 1, 0, 0)]);
}

#[tokio::test]
async fn composer_page_sorts_names_descending_with_ids_ascending() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let work = "a354b0c2-26c8-4b22-8226-0b9eaebc31be";
    let mut library = RecordsFixture::new().work(work, "Work Title A");
    // Two artists sharing a name, told apart by their MusicBrainz ids.
    for (composer, name) in [
        ("7aba84fe-423d-44f7-94b4-86bb73d0f49c", "Composer Name Shared"),
        ("4a571906-c332-438d-be58-8000905905cc", "Composer Name Shared"),
        ("d2b68adb-3657-460b-9d3b-95457125c403", "Composer Name Later"),
    ] {
        library = library
            .artist(composer, name)
            .catalog_ids(composer, None, Some(&format!("mb-{composer}")))
            .composer(work, composer);
    }
    library.write(&test_records(&db)).await;
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

/// [`next_album_page_is_only_a_rename`] for the composer grid: credit the one
/// composer `shown` holds on the work `work`, which it then counts.
async fn next_composer_page_is_only_a_work(
    records: &bae_records::Records,
    live: &mut coven::ReconfigurableLiveQuery<
        bae_store::LibraryPageWindows,
        ComposerBrowseProjection,
        bae_store::DbError,
    >,
    shown: ComposerBrowseProjection,
    work: &str,
) -> ComposerBrowseProjection {
    let composer_id = shown.windows[0].rows[0].artist.id.clone();
    RecordsFixture::new()
        .work(work, "Work Title Probe")
        .composer(work, &composer_id)
        .write(records)
        .await;
    let counted = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a work credited on the shown composer wakes composer browse")
        .expect("the query stays open")
        .result
        .unwrap();
    let mut expected = shown;
    expected.windows[0].rows[0].work_count += 1;
    assert_eq!(counted, expected, "the page carries more than the work");
    counted
}

/// The MusicBrainz artist the browse test's second composer is.
const SECOND_COMPOSER_MUSICBRAINZ: &str = "mb-composer-second";

/// The composer browse reads the windows asked for: another device naming a
/// shown composer otherwise moves it, and a write to a composer outside every
/// window delivers nothing.
#[test]
fn composer_browse_subscription_reconfigures_bounded_windows() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let db = devices.a();
        let records = test_records(db);
        let other = musicbrainz_artist(SECOND_COMPOSER_MUSICBRAINZ);
        devices.pause().await;
        RecordsFixture::new()
            .artist(COMPOSER_ID, "Composer Name First")
            .work(COMPOSER_WORK_ID, "Work Title First")
            .composer(COMPOSER_WORK_ID, COMPOSER_ID)
            .at("2026-01-02T00:00:00Z")
            .artist(&other, "Composer Name Second")
            .catalog_ids(&other, None, Some(SECOND_COMPOSER_MUSICBRAINZ))
            .work(OTHER_COMPOSER_WORK_ID, "Work Title Second")
            .composer(OTHER_COMPOSER_WORK_ID, &other)
            .write(&records)
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
        assert_eq!(both.windows[0].rows[0].artist.id, other);
        assert_eq!(both.windows[1].rows[0].artist.id, COMPOSER_ID);

        // B, apart, meets the second composer under another name; the later
        // write's name lands on A.
        meet_musicbrainz_artist(
            &test_records(devices.b()),
            SECOND_COMPOSER_MUSICBRAINZ,
            "Composer Name Renamed",
            None,
        )
        .await;
        devices.resume().await;
        let renamed_event = tokio::time::timeout(HANG_GUARD, async {
            loop {
                let event = live.next().await.expect("the query stays open");
                let shown = event.result.as_ref().unwrap();
                if shown.windows[0].rows[0].artist.name == "Composer Name Renamed" {
                    return event;
                }
            }
        })
        .await
        .expect("the other device's name wakes composer browse");
        assert_eq!(renamed_event.revision, both_pages_revision);
        let renamed = renamed_event.result.unwrap();
        assert_eq!(
            renamed
                .windows
                .iter()
                .map(|window| window.rows[0].artist.id.as_str())
                .collect::<Vec<_>>(),
            vec![other.as_str(), COMPOSER_ID]
        );
        devices
            .settle("both libraries", async |_, db| {
                test_records(db).find_artist_by_id(&other).await.unwrap().is_some()
                    && test_records(db).find_artist_by_id(COMPOSER_ID).await.unwrap().is_some()
            })
            .await;
        // A's image writes below leave their files on A alone, and their
        // versions name that device (`file@device`) until sync uploads the
        // file, which moves the version again. Stopping sync keeps each
        // version the one its write returned, so the events wait on writes
        // alone.
        devices.pause().await;

        requests.set([second_window].into_iter().collect::<BTreeSet<_>>()).unwrap();
        let hidden_other_composer =
            live.next().await.expect("the query stays open").result.unwrap();
        assert_eq!(hidden_other_composer.windows[0].rows[0].artist.id, COMPOSER_ID);

        // The edited composer is outside the requested window, so its sort
        // name, filled in by a credit naming it with one, changes nothing the
        // browse delivers.
        meet_musicbrainz_artist(
            &records,
            SECOND_COMPOSER_MUSICBRAINZ,
            "Composer Name Renamed",
            Some("Composer Sort Hidden"),
        )
        .await;
        let shown = next_composer_page_is_only_a_work(
            &records,
            &mut live,
            hidden_other_composer,
            "5f0b8a7e-2d5c-4e0b-9a51-7d1f0c3e2b01",
        )
        .await;

        let composer_image = write_artist_image(&records, &other, "composer image bytes").await;
        // Only the requested rows' images are read, so an image on the
        // composer outside the window changes nothing the browse delivers.
        next_composer_page_is_only_a_work(
            &records,
            &mut live,
            shown,
            "5f0b8a7e-2d5c-4e0b-9a51-7d1f0c3e2b02",
        )
        .await;
        let other_composer_page =
            [LibraryPageWindow { offset: 0, limit: 1 }].into_iter().collect::<BTreeSet<_>>();
        requests.set(other_composer_page).unwrap();
        let imaged = live.next().await.expect("the query stays open").result.unwrap();
        assert_eq!(imaged.windows[0].rows[0].artist.id, other);
        assert_eq!(
            imaged.windows[0].rows[0].image.as_ref().map(|image| image.version.as_str()),
            Some(composer_image.as_str())
        );

        let replaced_composer_image =
            write_artist_image(&records, &other, "replaced composer image bytes").await;
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

        RecordsFixture::new()
            .at("2026-01-03T00:00:00Z")
            .artist(INSERTED_COMPOSER_ID, "Composer Name Inserted")
            .work(INSERTED_COMPOSER_WORK_ID, "Work Title Inserted")
            .composer(INSERTED_COMPOSER_WORK_ID, INSERTED_COMPOSER_ID)
            .write(&records)
            .await;
        let inserted = live.next().await.expect("the query stays open").result.unwrap();
        assert_eq!(inserted.total_count, 3);
        assert!(inserted.windows.is_empty());
    });
}

/// The composer list on B is a local table B's triggers keep from the synced
/// rows: applying A's writes runs them, and B's live query delivers what they
/// wrote. A's work credits list their composers; the merge the two devices'
/// Discogs ids make then lists both works under the survivor alone.
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
            bae_store::DbError,
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

        RecordsFixture::new()
            .artist(COMPOSER_ID, "Composer Name First")
            .artist(OTHER_COMPOSER_ID, "Composer Name Second")
            .work(COMPOSER_WORK_ID, "Work Title First")
            .composer(COMPOSER_WORK_ID, COMPOSER_ID)
            .work(OTHER_COMPOSER_WORK_ID, "Work Title Second")
            .composer(OTHER_COMPOSER_WORK_ID, OTHER_COMPOSER_ID)
            .write(&test_records(writer))
            .await;
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

        merge_apart(devices, ("Composer Name First", "Composer Name Second"), "discogs-browse")
            .await;
        let merged = listed(&mut live, vec![(OTHER_COMPOSER_ID, 2)]).await;
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

const WORK_ACROSS_ALBUMS: &str = "55555555-0000-4000-8000-000000000001";
const ALBUM_ALPHA: &str = "11111111-0000-4000-8000-000000000002";
const RELEASE_BETA: &str = "22222222-0000-4000-8000-000000000001";
const RELEASE_ALPHA_PRIMARY: &str = "22222222-0000-4000-8000-000000000002";
const RELEASE_ALPHA_SECOND: &str = "22222222-0000-4000-8000-000000000004";
const RELEASE_ALPHA_THIRD: &str = "22222222-0000-4000-8000-000000000003";
const TRACK_ALPHA_PRIMARY: &str = "44444444-0000-4000-8000-000000000002";

/// `release`'s tracks: ones titled "Track Before" ahead of `track`, titled
/// `title`, its `number`th.
fn tracks_ending_in<'a>(
    release: &str,
    (track, title): (&'a str, &'a str),
    number: usize,
) -> Vec<(String, &'a str)> {
    let mut tracks: Vec<(String, &str)> = (1..number)
        .map(|place| (format!("4444{place:04}-0000-4000-8000-{}", &release[24..]), "Track Before"))
        .collect();
    tracks.push((track.to_string(), title));
    tracks
}

/// A work performed on releases of two albums, for the cover tests: `Album
/// Beta` performs it on its only release's first track; `album alpha`, first
/// by title compared without case, holds a primary release whose fourth track
/// doesn't perform it, then two that do, the earlier on its fifth track and
/// the later on its first.
fn work_across_albums() -> RecordsFixture {
    let album_beta = "11111111-0000-4000-8000-000000000001";
    let mut library = RecordsFixture::new()
        .album(album_beta, "Album Beta", Some(2026), &[])
        .album(ALBUM_ALPHA, "album alpha", Some(2026), &[])
        .release(album_beta, RELEASE_BETA)
        .tracks(RELEASE_BETA, &[("44444444-0000-4000-8000-000000000001", "Beta Track")])
        .work(WORK_ACROSS_ALBUMS, "Work Across Albums")
        .performs("44444444-0000-4000-8000-000000000001", WORK_ACROSS_ALBUMS);
    for (release, track, title, number, performs) in [
        (RELEASE_ALPHA_PRIMARY, TRACK_ALPHA_PRIMARY, "Alpha Primary Track", 4, false),
        (
            RELEASE_ALPHA_SECOND,
            "44444444-0000-4000-8000-000000000004",
            "Alpha Second Track",
            5,
            true,
        ),
        (RELEASE_ALPHA_THIRD, "44444444-0000-4000-8000-000000000003", "Alpha Third Track", 1, true),
    ] {
        let tracks = tracks_ending_in(release, (track, title), number);
        let tracks: Vec<(&str, &str)> =
            tracks.iter().map(|(id, title)| (id.as_str(), *title)).collect();
        library = library.release(ALBUM_ALPHA, release).tracks(release, &tracks);
        if performs {
            library = library.performs(track, WORK_ACROSS_ALBUMS);
        }
    }
    library
}

/// [`work_across_albums`]'s library, written.
async fn work_across_albums_db(library: RecordsFixture) -> (Store, tempfile::TempDir) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    library.write(&test_records(&db)).await;
    (db, tmp)
}

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
    let (db, _tmp) = work_across_albums_db(work_across_albums()).await;
    assert_eq!(representative_release(&db).await.as_deref(), Some(RELEASE_ALPHA_SECOND));
}

/// When the first album's primary release performs the work, its cover is the
/// work's, though a later release performs the work on an earlier track.
#[tokio::test]
async fn a_works_cover_is_its_first_albums_primary_release_when_it_performs_it() {
    let (db, _tmp) = work_across_albums_db(
        work_across_albums().performs(TRACK_ALPHA_PRIMARY, WORK_ACROSS_ALBUMS),
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
    let album_twin = "11111111-0000-4000-8000-000000000003";
    let release_twin = "22222222-0000-4000-8000-000000000005";
    let track_twin = "44444444-0000-4000-8000-000000000005";
    let (db, _tmp) = work_across_albums_db(
        work_across_albums()
            .album(album_twin, "album alpha", Some(2026), &[])
            .release(album_twin, release_twin)
            .tracks(release_twin, &[(track_twin, "Alpha Twin Track")])
            .performs(track_twin, WORK_ACROSS_ALBUMS),
    )
    .await;

    let detail = crate::LibraryViews::new(db.clone())
        .find_work_detail(WORK_ACROSS_ALBUMS)
        .await
        .unwrap()
        .expect("the work is in the library");

    let in_order = [RELEASE_ALPHA_SECOND, RELEASE_ALPHA_THIRD, release_twin, RELEASE_BETA];
    assert_eq!(
        detail.releases.iter().map(|release| release.release_id.as_str()).collect::<Vec<_>>(),
        in_order
    );
    assert_eq!(
        detail.tracks.iter().map(|track| track.track.release_id.as_str()).collect::<Vec<_>>(),
        in_order
    );
}

/// Only the writing credits of a work make an artist its composer: an artist
/// the work credits as its arranger is no composer, counts for no work, and
/// does not name the work.
#[tokio::test]
async fn composer_list_counts_the_writing_credits_of_a_work_only() {
    use bae_types::credit_role::{CreditRole, CreditRoleKind, CreditedAs, RoleQualifier};
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    listed_releases()
        .artist(LISTED_SURVIVOR, "Composer Name Writer")
        .artist(LISTED_ABSORBED, "Composer Name Arranger")
        .work(LISTED_WORK_ONE, "Work Title One")
        .work_credit(LISTED_WORK_ONE, LISTED_SURVIVOR, CreditedAs::of(CreditRoleKind::Writer))
        .work_credit(
            LISTED_WORK_ONE,
            LISTED_ABSORBED,
            CreditedAs {
                role: CreditRole::Kind(CreditRoleKind::Arranger),
                qualifiers: vec![RoleQualifier::Stated("lead vocals".to_string())],
            },
        )
        .performs(LISTED_TRACK_ONE, LISTED_WORK_ONE)
        .write(&test_records(&db))
        .await;

    assert_eq!(listed_composers(&db).await, vec![(LISTED_SURVIVOR.to_string(), 1, 1, 0)]);
    let results =
        crate::LibraryViews::new(db.clone()).search_rows("Work Title One", 10).await.unwrap();
    assert_eq!(
        results.works.first().expect("the work").composer_names.as_deref(),
        Some("Composer Name Writer")
    );
}
