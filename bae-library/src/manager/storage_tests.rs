//! The storage list: its sorting, filters, pages, counts and sizes, the actions
//! each row offers, and the queued counts each release's storage row keeps.

use super::*;
use crate::manager::tests::*;
use bae_records::{DbAlbum, DbRelease, NewReleaseFile};
use chrono::Utc;

/// Two release ids whose lexical order is fixed, for the `r.id` tiebreaker.
const TIEBREAK_LO: &str = "066483e0-f9fc-4636-865d-08c069510b2e";

const TIEBREAK_HI: &str = "2153cb27-8335-4523-ae52-be2d6f577ba3";

#[tokio::test]
async fn storage_page_id_tiebreaker_stable_across_pages() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    // Two releases sharing album title + created_at — the ORDER BY clause
    // falls through to the `r.id` tiebreaker. The ids are canonical UUIDs
    // (coven takes no other shape on a synced row) chosen so LO sorts first.
    let now = Utc::now();
    let mut album = create_test_album();
    album.title = "Same Title".to_string();
    manager.records.insert_album(&album).await.unwrap();
    let mut release_a = create_test_release(&album.id);
    release_a.id = TIEBREAK_LO.to_string();
    release_a.created_at = now;
    let mut release_b = create_test_release(&album.id);
    release_b.id = TIEBREAK_HI.to_string();
    release_b.created_at = now;
    manager.records.insert_release(&release_a).await.unwrap();
    manager.records.insert_release(&release_b).await.unwrap();

    let sort = bae_views::StorageSortCriterion {
        field: bae_views::StorageSortField::AlbumTitle,
        direction: bae_store::SortDirection::Ascending,
    };
    let first_page =
        views(manager).get_storage_page(&sort, bae_views::StorageFilter::All, 0, 1).await.unwrap();
    let second_page =
        views(manager).get_storage_page(&sort, bae_views::StorageFilter::All, 1, 1).await.unwrap();

    assert_eq!(first_page.rows.len(), 1);
    assert_eq!(second_page.rows.len(), 1);
    assert_eq!(first_page.rows[0].release.id, TIEBREAK_LO);
    assert_eq!(second_page.rows[0].release.id, TIEBREAK_HI);
}

// The queued count and first queued place `release_storage` keeps per
// release, and the queued count `release_listing` copies, against those
// probed from coven's `coven_uploads` file by file, through random library
// writes and uploads: moving releases to the cloud, completing and cancelling
// uploads, retiring one by deleting its file's row, a file moving between
// releases, covers queued beside them, and deletes.

/// Each release whose storage row's queued count or first place differs
/// from the ones probed from its files in coven's queue, or that has no
/// storage row, and each whose listing row is missing or holds a queued
/// count other than its storage row's: none when the triggers kept up.
async fn queue_storage_disagreements(manager: &LibraryManager) -> Vec<String> {
    manager
        .store
        .query_texts_for_test(
            "SELECT r.id || ': stored ' || COALESCE(s.queued_count || ' from ' \
                     || COALESCE(s.first_queued_position, 'nowhere'), 'nothing') \
                 || ', listed ' || COALESCE(l.queued_count, 'nothing') \
                 || ', probed ' || probe.queued || ' from ' || COALESCE(probe.first, 'nowhere') \
             FROM releases r \
             LEFT JOIN release_storage s ON s.release_id = r.id \
             LEFT JOIN release_listing l ON l.release_id = r.id \
             JOIN (SELECT r2.id AS release_id, \
                     (SELECT COUNT(*) FROM release_files f JOIN coven_uploads u \
                        ON u.file = f.file \
                      WHERE f.release_id = r2.id) AS queued, \
                     (SELECT MIN(u.position) FROM release_files f JOIN coven_uploads u \
                        ON u.file = f.file \
                      WHERE f.release_id = r2.id) AS first \
                   FROM releases r2) probe ON probe.release_id = r.id \
             WHERE s.release_id IS NULL OR s.queued_count <> probe.queued \
                OR s.first_queued_position IS NOT probe.first \
                OR l.release_id IS NULL OR l.queued_count <> s.queued_count",
        )
        .await
        .unwrap()
}

/// The releases with files in coven's queue, in the order their first files
/// wait, probed file by file.
async fn probed_uploading_releases(manager: &LibraryManager) -> Vec<String> {
    manager
        .store
        .query_texts_for_test(
            "SELECT f.release_id FROM coven_uploads u \
             JOIN release_files f ON u.file = f.file \
             GROUP BY f.release_id ORDER BY MIN(u.position)",
        )
        .await
        .unwrap()
}

/// Random writes against the probe: after each, every release's queued
/// count and first place in its storage row, and the queued count its
/// listing row copies, are the probed ones, and so are the releases
/// uploading, in the order read from them.
#[tokio::test]
async fn the_listed_queue_counts_follow_every_upload_and_file_write() {
    use rand::{Rng, SeedableRng};
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(0x9e7e);
    let mut releases: Vec<HarnessRelease> = Vec::new();
    let mut ran: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut most_uploading = 0usize;
    for step in 0..400 {
        let pick = |rng: &mut rand::rngs::StdRng, releases: &[HarnessRelease], stage| {
            let matching: Vec<usize> =
                (0..releases.len()).filter(|&index| releases[index].stage == stage).collect();
            (!matching.is_empty()).then(|| matching[rng.random_range(0..matching.len())])
        };
        let op = match rng.random_range(0..8) {
            0 | 1 if releases.len() < 6 => {
                let files: Vec<(String, Vec<u8>)> = (0..rng.random_range(1..=3))
                    .map(|index| (format!("{index}.flac"), format!("{step}-{index}").into_bytes()))
                    .collect();
                let files: Vec<(&str, &[u8])> =
                    files.iter().map(|(name, bytes)| (name.as_str(), bytes.as_slice())).collect();
                let release = insert_local_release_with_files(
                    manager,
                    &temp_dir.path().join(format!("release-{step}")),
                    &format!("Album Title {step}"),
                    &files,
                )
                .await;
                releases.push(HarnessRelease { id: release.id, stage: HarnessStage::Local });
                "add release"
            }
            2 | 3 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Local) else {
                    continue;
                };
                manager.upload_release(&releases[index].id, false).await.unwrap();
                releases[index].stage = HarnessStage::Attached;
                "attach"
            }
            4 => {
                if !releases.iter().any(|release| release.stage == HarnessStage::Attached) {
                    continue;
                }
                manager.drain_uploads_expecting_work().await.unwrap();
                for release in &mut releases {
                    if release.stage == HarnessStage::Attached {
                        release.stage = HarnessStage::Remote;
                    }
                }
                "complete uploads"
            }
            5 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Attached) else {
                    continue;
                };
                manager.cancel_release_upload(&releases[index].id).await.unwrap();
                releases[index].stage = HarnessStage::Local;
                "cancel upload"
            }
            6 => {
                if releases.is_empty() {
                    continue;
                }
                let index = rng.random_range(0..releases.len());
                store_test_cover(manager, &releases[index].id, format!("cover {step}").as_bytes())
                    .await;
                "queue cover"
            }
            7 if !releases.is_empty() => {
                let index = rng.random_range(0..releases.len());
                let release = releases.remove(index);
                manager.delete_release(&release.id).await.unwrap();
                "delete release"
            }
            _ => continue,
        };
        *ran.entry(op).or_default() += 1;

        assert_eq!(
            queue_storage_disagreements(manager).await,
            Vec::<String>::new(),
            "after step {step}, {op}"
        );
        let probed = probed_uploading_releases(manager).await;
        most_uploading = most_uploading.max(probed.len());
        assert_eq!(
            manager.records.uploading_releases().await.unwrap(),
            probed,
            "the releases uploading after step {step}, {op}"
        );
    }
    for op in [
        "add release",
        "attach",
        "complete uploads",
        "cancel upload",
        "queue cover",
        "delete release",
    ] {
        assert!(ran.contains_key(op), "the walk never ran {op}: {ran:?}");
    }
    assert!(most_uploading > 1, "never more than one release uploading");
}

/// Each storage-page row carries its own release's cover reference, not the
/// album's primary-release cover. Two releases of one album, each with its own
/// `covers` row, resolve to their own ids; a non-primary release's row resolves
/// to its own cover rather than the album's primary.
#[tokio::test]
async fn storage_page_rows_carry_each_releases_own_cover() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release1).await;
    insert_release(manager, &release2).await;
    // release1 is the album's primary, so its cover is the album-level cover.
    manager.records.set_album_primary_release(&album.id, &release1.id).await.unwrap();

    store_test_cover(manager, &release1.id, b"cover").await;
    store_test_cover(manager, &release2.id, b"cover").await;

    let page = views(manager)
        .get_storage_page(
            &bae_views::StorageSortCriterion {
                field: bae_views::StorageSortField::AlbumTitle,
                direction: bae_store::SortDirection::Ascending,
            },
            bae_views::StorageFilter::All,
            0,
            100,
        )
        .await
        .unwrap();

    let row1 =
        page.rows.iter().find(|r| r.release.id == release1.id).expect("release1 row present");
    let row2 =
        page.rows.iter().find(|r| r.release.id == release2.id).expect("release2 row present");

    // Each release row resolves to that release's own cover.
    assert_eq!(row1.release.cover.as_ref().unwrap().id, release1.id);
    assert_eq!(row2.release.cover.as_ref().unwrap().id, release2.id);

    // The album carries the primary release's cover; the non-primary release's
    // row carries its own, distinct from the album's.
    assert_eq!(row2.album.cover.as_ref().unwrap().id, release1.id);
    assert_ne!(row2.release.cover.as_ref().unwrap().id, row2.album.cover.as_ref().unwrap().id);
}

/// Insert N albums each with one release; return `(albums, releases)`.
/// Each album's title is `"Album {i}"` so ordering is deterministic.
async fn seed_albums(manager: &LibraryManager, count: usize) -> (Vec<DbAlbum>, Vec<DbRelease>) {
    let mut albums = Vec::new();
    let mut releases = Vec::new();
    for i in 0..count {
        let mut album = create_test_album();
        album.title = format!("Album {i}");
        let release = create_test_release(&album.id);
        manager.records.insert_album(&album).await.unwrap();
        insert_release(manager, &release).await;
        albums.push(album);
        releases.push(release);
    }
    (albums, releases)
}

#[tokio::test]
async fn storage_page_returns_all_rows_for_all_filter() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    seed_albums(manager, 3).await;

    let page = storage_page(manager, bae_views::StorageFilter::All, 0, 10).await;
    assert_eq!(page.rows.len(), 3);
    assert_eq!(page.total_count, 3);
}

#[tokio::test]
async fn storage_page_paginates() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    seed_albums(manager, 5).await;

    let page1 = storage_page(manager, bae_views::StorageFilter::All, 0, 2).await;
    let page2 = storage_page(manager, bae_views::StorageFilter::All, 2, 2).await;
    let page3 = storage_page(manager, bae_views::StorageFilter::All, 4, 2).await;

    assert_eq!(page1.rows.len(), 2);
    assert_eq!(page2.rows.len(), 2);
    assert_eq!(page3.rows.len(), 1);
    // total_count is the full filtered universe, not the page.
    assert_eq!(page1.total_count, 5);
    assert_eq!(page2.total_count, 5);
    assert_eq!(page3.total_count, 5);
}

#[tokio::test]
async fn storage_page_sorts_album_title_ascending() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    seed_albums(manager, 3).await;

    let page = storage_page(manager, bae_views::StorageFilter::All, 0, 10).await;
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album 0", "Album 1", "Album 2"]);
}

#[tokio::test]
async fn storage_page_sorts_album_title_descending() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    seed_albums(manager, 3).await;

    let sort = bae_views::StorageSortCriterion {
        field: bae_views::StorageSortField::AlbumTitle,
        direction: bae_store::SortDirection::Descending,
    };
    let page =
        views(manager).get_storage_page(&sort, bae_views::StorageFilter::All, 0, 10).await.unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album 2", "Album 1", "Album 0"]);
}

/// Each storage-page row carries the state-appropriate `storage_actions`
/// the Storage Manager row context menu renders — pinned offers unpin +
/// audio detachment, cloud-only offers pin + audio detachment, local offers upload.
/// With storage set up every remote/local transition is open.
#[tokio::test]
async fn storage_page_rows_carry_state_appropriate_actions() {
    use bae_types::release_storage::ReleaseStorageAction::{DetachAudio, Pin, Unpin, Upload};

    let (test, temp_dir) = setup_test_manager().await;

    let manager = test.library.manager();
    connect_test_cloud_uploading(&test).await;

    // Pinned: made Remote with pin, so its file lands in coven's offline cache.
    let pinned =
        upload_release_unpaused(manager, &temp_dir.path().join("pinned"), "Pinned Album", true)
            .await;
    // Cloud-only: made Remote without pin, so its file is evictable, not pinned.
    let cloud_only =
        upload_release_unpaused(manager, &temp_dir.path().join("cloud"), "Cloud Album", false)
            .await;

    // Local: not remote, files at a local path.
    let mut local_album = create_test_album();
    local_album.title = "Local Album".to_string();
    let local = create_test_release(&local_album.id);
    manager.records.insert_album(&local_album).await.unwrap();
    insert_release(manager, &local).await;

    let page = storage_page(manager, bae_views::StorageFilter::All, 0, 10).await;
    let actions: std::collections::HashMap<_, _> = page
        .rows
        .iter()
        .map(|r| (r.release.id.clone(), r.release.storage_actions.clone()))
        .collect();

    assert_eq!(actions[&pinned], vec![Unpin, DetachAudio]);
    assert_eq!(actions[&cloud_only], vec![Pin, DetachAudio]);
    assert_eq!(actions[&local.id], vec![Upload]);
}

/// With no storage, no remote storage exists, so the rows offer no
/// transitions — the context menu is empty everywhere.
#[tokio::test]
async fn storage_page_rows_have_no_actions_without_storage() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;

    let page = storage_page(manager, bae_views::StorageFilter::All, 0, 10).await;
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].release.storage_actions.is_empty());
}

#[tokio::test]
async fn storage_page_local_filter_matches_local_path() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let remote_id =
        uploaded_release(manager, &temp_dir.path().join("remote"), "Remote Album", false).await;
    let local_release = insert_local_release_with_files(
        manager,
        &temp_dir.path().join("local"),
        "Local Album",
        &[("track.flac", b"local")],
    )
    .await;

    let local = storage_page(manager, bae_views::StorageFilter::Local, 0, 10).await;
    assert_eq!(local.rows.len(), 1);
    assert_eq!(local.total_count, 1);
    assert_eq!(local.rows[0].release.id, local_release.id);
    assert_eq!(
        local.rows[0].release.storage_state,
        bae_views::album_detail::ReleaseStorageState::OnOneDevice
    );

    let remote = storage_page(manager, bae_views::StorageFilter::Remote, 0, 10).await;
    assert_eq!(remote.rows.len(), 1);
    assert_eq!(remote.rows[0].release.id, remote_id);
    assert_ne!(
        remote.rows[0].release.storage_state,
        bae_views::album_detail::ReleaseStorageState::OnOneDevice
    );
}

#[tokio::test]
async fn storage_count_matches_filtered_page_total() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let mut inserted_local = None;
    for i in 0..3 {
        let release = insert_local_release_with_files(
            manager,
            &temp_dir.path().join(format!("album-{i}")),
            &format!("Album {i}"),
            &[("track.flac", b"track")],
        )
        .await;
        if i == 1 {
            inserted_local = Some(release.id);
        } else {
            manager.upload_release(&release.id, false).await.unwrap();
            manager.drain_uploads_expecting_work().await.unwrap();
        }
    }

    assert_eq!(views(manager).get_storage_count(bae_views::StorageFilter::All).await.unwrap(), 3);
    assert_eq!(views(manager).get_storage_count(bae_views::StorageFilter::Local).await.unwrap(), 1);
    assert_eq!(
        views(manager).get_storage_count(bae_views::StorageFilter::Remote).await.unwrap(),
        2
    );

    let all_page = storage_page(manager, bae_views::StorageFilter::All, 0, 10).await;
    assert_eq!(all_page.total_count, 3);

    let local_page = storage_page(manager, bae_views::StorageFilter::Local, 0, 10).await;
    assert_eq!(local_page.rows.len(), 1);
    assert_eq!(local_page.rows[0].release.id, inserted_local.unwrap());
}

/// `get_storage_total_size` sums `total_size` over every storage row matching
/// `filter` — the same universe `get_storage_page` pages over — independent of
/// how many pages have loaded. For each filter, the aggregate must equal the
/// sum of `total_size` over that filter's full (unpaginated) storage page.
#[tokio::test]
async fn storage_total_size_matches_page_total_size_sum() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;

    insert_local_release_with_files(
        manager,
        &temp_dir.path().join("local"),
        "Local Album",
        &[("a.flac", &vec![0u8; 1_000])],
    )
    .await;
    uploaded_release_with_files(
        manager,
        &temp_dir.path().join("remote"),
        "Remote Album",
        &[("a.flac", &[0u8; 100])],
        false,
    )
    .await;
    insert_release_with_queued_uploads(
        manager,
        &temp_dir.path().join("uploading"),
        "Uploading Album",
        &[("a.flac", &vec![0u8; 10_000])],
    )
    .await;

    for filter in [
        bae_views::StorageFilter::All,
        bae_views::StorageFilter::Local,
        bae_views::StorageFilter::Remote,
    ] {
        let page = views(manager)
            .get_storage_page(&sort_by_album_title_asc(), filter, 0, 10)
            .await
            .unwrap();
        let page_sum: i64 = page.rows.iter().map(|row| row.release.total_size).sum();

        let aggregate = views(manager).get_storage_total_size(filter).await.unwrap();
        assert_eq!(
            aggregate, page_sum as u64,
            "{filter:?}: aggregate must equal the page's own total_size sum"
        );
    }

    // A release stays local while its upload is pending: its rows name no
    // file until storage holds it.
    assert_eq!(
        views(manager).get_storage_total_size(bae_views::StorageFilter::All).await.unwrap(),
        11_100
    );
    assert_eq!(
        views(manager).get_storage_total_size(bae_views::StorageFilter::Local).await.unwrap(),
        11_000
    );
    assert_eq!(
        views(manager).get_storage_total_size(bae_views::StorageFilter::Remote).await.unwrap(),
        100
    );
}

#[tokio::test]
async fn storage_page_sort_by_artist_names() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    // Two artists with distinct sort-orderings; ArtistNames sort triggers
    // the `needs_artist_sort_join` branch.
    for (artist_id, artist_name) in &[
        ("ba5b6a6c-bc8c-4015-8b3c-03e78dfe28e5", "Zulu"),
        ("f2ad46f1-3a5e-4bb5-807f-5a314ae94f25", "Alpha"),
    ] {
        let artist = DbArtist {
            id: artist_id.to_string(),
            name: artist_name.to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        };
        manager.records.insert_artist(&artist).await.unwrap();
        let mut album = create_test_album();
        album.title = format!("Album by {artist_name}");
        let release = create_test_release(&album.id);
        manager.records.insert_album(&album).await.unwrap();
        manager
            .records
            .insert_album_artist(&bae_records::DbAlbumArtist::new_test(&album.id, artist_id))
            .await
            .unwrap();
        insert_release(manager, &release).await;
    }

    let asc = bae_views::StorageSortCriterion {
        field: bae_views::StorageSortField::ArtistNames,
        direction: bae_store::SortDirection::Ascending,
    };
    let page =
        views(manager).get_storage_page(&asc, bae_views::StorageFilter::All, 0, 10).await.unwrap();
    let names: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(names, vec!["Album by Alpha", "Album by Zulu"]);

    let desc = bae_views::StorageSortCriterion {
        field: bae_views::StorageSortField::ArtistNames,
        direction: bae_store::SortDirection::Descending,
    };
    let page =
        views(manager).get_storage_page(&desc, bae_views::StorageFilter::All, 0, 10).await.unwrap();
    let names: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(names, vec!["Album by Zulu", "Album by Alpha"]);
}

#[tokio::test]
async fn storage_page_sort_by_media_nulls_last() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    for (title, media) in &[
        ("Album No Media", None),
        ("Album CD", Some(bae_types::pressing::Medium::Cd)),
        ("Album Vinyl", Some(bae_types::pressing::Medium::Vinyl)),
    ] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let mut release = create_test_release(&album.id);
        release.pressing.facts =
            media.map(|medium| bae_types::pressing::made_of(medium, 1)).unwrap_or_default();
        manager.records.insert_album(&album).await.unwrap();
        insert_release(manager, &release).await;
    }

    let asc = bae_views::StorageSortCriterion {
        field: bae_views::StorageSortField::Media,
        direction: bae_store::SortDirection::Ascending,
    };
    let page =
        views(manager).get_storage_page(&asc, bae_views::StorageFilter::All, 0, 10).await.unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    // Unknown media sorts last in both directions.
    assert_eq!(titles, vec!["Album CD", "Album Vinyl", "Album No Media"]);

    let desc = bae_views::StorageSortCriterion {
        field: bae_views::StorageSortField::Media,
        direction: bae_store::SortDirection::Descending,
    };
    let page =
        views(manager).get_storage_page(&desc, bae_views::StorageFilter::All, 0, 10).await.unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album Vinyl", "Album CD", "Album No Media"]);
}

#[tokio::test]
async fn storage_page_sort_by_file_count() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    // Three releases, each with a distinct number of files.
    for (title, file_count) in &[("Album A", 1usize), ("Album B", 3), ("Album C", 2)] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let release = create_test_release(&album.id);
        manager.records.insert_album(&album).await.unwrap();
        insert_release(manager, &release).await;
        for i in 0..*file_count {
            let file = NewReleaseFile {
                id: bae_util::test_ids::test_uuid(&format!("{}-file-{i}", release.id)),
                release_id: release.id.clone(),
                original_filename: format!("{i}.flac"),
                content_type: bae_util::content_type::ContentType::Flac,
                role: bae_records::ReleaseFileRole::Other,
                source_audio: None,
                created_at: Utc::now(),
            };
            manager.add_file(&file, 1000).await.unwrap();
        }
    }

    let desc = bae_views::StorageSortCriterion {
        field: bae_views::StorageSortField::FileCount,
        direction: bae_store::SortDirection::Descending,
    };
    let page =
        views(manager).get_storage_page(&desc, bae_views::StorageFilter::All, 0, 10).await.unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album B", "Album C", "Album A"]);
}

#[tokio::test]
async fn storage_page_sort_by_total_size() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();

    for (title, file_size) in &[("Small", 100u64), ("Big", 10_000), ("Medium", 1_000)] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let release = create_test_release(&album.id);
        manager.records.insert_album(&album).await.unwrap();
        insert_release(manager, &release).await;
        let file = NewReleaseFile {
            id: bae_util::test_ids::test_uuid(&format!("{}-file", release.id)),
            release_id: release.id.clone(),
            original_filename: "a.flac".to_string(),
            content_type: bae_util::content_type::ContentType::Flac,
            role: bae_records::ReleaseFileRole::Other,
            source_audio: None,
            created_at: Utc::now(),
        };
        manager.add_file(&file, *file_size).await.unwrap();
    }

    let asc = bae_views::StorageSortCriterion {
        field: bae_views::StorageSortField::TotalSize,
        direction: bae_store::SortDirection::Ascending,
    };
    let page =
        views(manager).get_storage_page(&asc, bae_views::StorageFilter::All, 0, 10).await.unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Small", "Medium", "Big"]);
}

/// Sorting by date added puts the releases in the order they were added,
/// newest first descending; releases added at the same moment go by album
/// title either way. Each row carries when its release was added.
#[tokio::test]
async fn storage_page_sort_by_date_added() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let at = |seconds: i64| chrono::DateTime::from_timestamp(1_700_000_000 + seconds, 0).unwrap();
    for (title, added) in &[
        ("Album First", at(0)),
        ("Album Last", at(20)),
        ("Album Tied B", at(10)),
        ("Album Tied A", at(10)),
    ] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let mut release = create_test_release(&album.id);
        release.created_at = *added;
        manager.records.insert_album(&album).await.unwrap();
        insert_release(manager, &release).await;
    }

    let page = |direction| {
        let manager = &manager;
        async move {
            let sort = bae_views::StorageSortCriterion {
                field: bae_views::StorageSortField::DateAdded,
                direction,
            };
            views(manager)
                .get_storage_page(&sort, bae_views::StorageFilter::All, 0, 10)
                .await
                .unwrap()
                .rows
                .into_iter()
                .map(|row| (row.album.title, row.date_added))
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(
        page(bae_store::SortDirection::Ascending).await,
        vec![
            ("Album First".to_string(), at(0)),
            ("Album Tied A".to_string(), at(10)),
            ("Album Tied B".to_string(), at(10)),
            ("Album Last".to_string(), at(20)),
        ]
    );
    assert_eq!(
        page(bae_store::SortDirection::Descending).await,
        vec![
            ("Album Last".to_string(), at(20)),
            ("Album Tied A".to_string(), at(10)),
            ("Album Tied B".to_string(), at(10)),
            ("Album First".to_string(), at(0)),
        ]
    );
}

/// Sorting by storage puts the releases uploading first (a failed download
/// before them), then those waiting
/// or downloading to be pinned, then the local, the pinned and the
/// cloud-only ones, whatever their titles; descending reverses it, and each
/// filter keeps the order among the releases it shows.
#[tokio::test]
async fn storage_page_sort_by_storage_state() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let dir = |name: &str| temp_dir.path().join(name);

    let cloud = uploaded_release(manager, &dir("cloud"), "Album Title A", false).await;
    let pinned = uploaded_release(manager, &dir("pinned"), "Album Title B", true).await;
    let local = insert_local_release_with_files(
        manager,
        &dir("local"),
        "Album Title C",
        &[("track.flac", b"local")],
    )
    .await
    .id;
    let pinning = uploaded_release(manager, &dir("pinning"), "Album Title D", false).await;
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![pinning.clone()]).await.unwrap();
    let uploading = insert_release_with_queued_uploads(
        manager,
        &dir("uploading"),
        "Album Title E",
        &[("track.flac", b"uploading")],
    )
    .await
    .id;

    use bae_store::SortDirection;
    use bae_views::StorageFilter;
    let ascending =
        vec![uploading.clone(), pinning.clone(), local.clone(), pinned.clone(), cloud.clone()];
    assert_eq!(by_storage(manager, StorageFilter::All, SortDirection::Ascending).await, ascending);
    let sort = bae_views::StorageSortCriterion {
        field: bae_views::StorageSortField::Storage,
        direction: SortDirection::Ascending,
    };
    use bae_views::album_detail::StorageRowStatus;
    assert_eq!(
        views(manager)
            .get_storage_page(&sort, StorageFilter::All, 0, 10)
            .await
            .unwrap()
            .rows
            .iter()
            .map(|row| row.status.clone())
            .collect::<Vec<_>>(),
        vec![
            StorageRowStatus::Uploading,
            StorageRowStatus::Pinning(bae_views::album_detail::DownloadEntryStep::Queued),
            StorageRowStatus::OnOneDevice,
            StorageRowStatus::Pinned,
            StorageRowStatus::InStorage,
        ],
        "each row shows where it sorts"
    );
    let mut descending = ascending.clone();
    descending.reverse();
    assert_eq!(
        by_storage(manager, StorageFilter::All, SortDirection::Descending).await,
        descending
    );
    assert_eq!(
        by_storage(manager, StorageFilter::Local, SortDirection::Ascending).await,
        vec![uploading.clone(), local.clone()]
    );
    assert_eq!(
        by_storage(manager, StorageFilter::Remote, SortDirection::Ascending).await,
        vec![pinning.clone(), pinned.clone(), cloud.clone()]
    );
}
