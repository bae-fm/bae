//! What writes cost at M, and what each family of triggers adds to them. Each
//! write is the median of 15 distinct writes through the owning domain's own
//! writes. The scenarios run on a copy of the seed with all its triggers
//! (`write/<scenario>@M`) and again on a copy with one family dropped
//! (`write/<scenario>/without=<family>@M`); the difference is that family's
//! cost on that write.
//!
//! Not measured: renaming an artist and merging two (no write of the records
//! does either alone), a rating and a playlist entry (they need a library's
//! person), a file stored, pinned or upload-done (they need coven storage).

use super::seed::{SEEDS, SIZES};
use bae_catalogs::catalog::Catalog;
use bae_records::test_fixtures::library::LibraryRows;
use bae_records::test_fixtures::shaped_library;
use bae_records::{DbTrackArtistRole, DbTrackWork, NewReleaseFile, ReleaseFileRole};
use bae_store::test_fixtures::screen_cost::{prepared_write_cost, write_cost};
use bae_store::Store;
use bae_types::credit_role::{CreditRoleKind, CreditedAs};
use std::path::Path;

/// M: the writes are measured on the 20,000-album library.
const M: u64 = SIZES[1];

/// A family of triggers: its name in the metrics, and the owning crate's
/// function that drops it from a closed library file.
/// The function dropping a family from a closed library's folder.
type DropFamily = fn(&Path);

const FAMILIES: &[(&str, DropFamily)] = &[
    ("album-listing", bae_views::trigger_families::drop_album_listing_triggers),
    ("release-listing", bae_views::trigger_families::drop_release_listing_triggers),
    ("composer", bae_views::trigger_families::drop_composer_triggers),
    ("composer-detail", bae_views::trigger_families::drop_composer_detail_triggers),
    ("search", bae_views::trigger_families::drop_search_triggers),
    ("artist-listing", bae_views::trigger_families::drop_artist_listing_triggers),
    ("release-storage", bae_records::test_fixtures::drop_release_storage_triggers),
    ("transfer-queues", bae_transfers::tables::bulk::drop_queue_triggers),
];

/// Time writing `count` albums, each the next one `want` accepts from
/// `next` on, one write each; the albums between are written first, untimed:
/// albums are written in order, since each refers to the artists and works
/// the ones before it wrote.
async fn timed_albums(
    db: &Store,
    metric: &str,
    next: &mut u64,
    count: usize,
    want: impl Fn(u64) -> bool,
) {
    let at = std::cell::Cell::new(*next);
    prepared_write_cost(
        metric,
        count,
        |_| {
            let from = at.get();
            let album = (from..).find(|i| want(*i)).expect("an album that fits");
            at.set(album + 1);
            let db = db.clone();
            async move {
                if from < album {
                    db.write(move |sql| shaped_library::insert_on(&sql, from, album))
                        .await
                        .unwrap();
                }
                album
            }
        },
        |album| {
            let db = db.clone();
            async move {
                db.write(move |sql| shaped_library::insert_on(&sql, album, album + 1))
                    .await
                    .unwrap();
            }
        },
    )
    .await;
    *next = at.get();
}

async fn scenarios(db: &Store, label: &str) {
    let records = bae_records::test_fixtures::test_records(db);
    let suffix = |name: &str| format!("write/{name}{label}@M");
    let mut next = M;
    // The library imported: one release of 12 tracks, then the box set.
    timed_albums(db, &suffix("import-12-tracks"), &mut next, 15, |i| {
        shaped_library::track_count(i) == 12 && shaped_library::release_count(i) == 1
    })
    .await;
    timed_albums(db, &suffix("import-600-tracks"), &mut next, 5, |i| {
        shaped_library::track_count(i) == 600
    })
    .await;
    let base = M;
    // A release deleted.
    let mut deleted = 0;
    write_cost(&suffix("delete-release"), |_| {
        let records = records.clone();
        let release = shaped_library::release_id(base + 1 + deleted * 2, 0);
        deleted += 1;
        async move {
            bae_records::test_fixtures::delete_release(&records, &release).await;
        }
    })
    .await;
    // An album retitled.
    let mut retitled = 0;
    write_cost(&suffix("retitle-album"), |run| {
        let records = records.clone();
        let album = shaped_library::album_id(1_000 + retitled * 7);
        retitled += 1;
        async move {
            records.rename_album_for_test(&album, &format!("Retitled {run}")).await.unwrap();
        }
    })
    .await;
    // A work added to a track, and a role credit to another.
    write_cost(&suffix("add-work-to-track"), |run| {
        let records = records.clone();
        async move {
            let mut rows = LibraryRows::default();
            rows.track_works.push(DbTrackWork::new(
                &shaped_library::track_id(2_000 + run * 3, 0, 0),
                &shaped_library::work_id(run),
                9,
                Catalog::MusicBrainz,
                format!("{run:08x}-0009-4000-8000-{:012x}", 777),
                chrono::Utc::now(),
            ));
            records.insert_library_rows_for_test(rows).await.unwrap();
        }
    })
    .await;
    write_cost(&suffix("add-track-role-credit"), |run| {
        let records = records.clone();
        async move {
            let mut rows = LibraryRows::default();
            rows.track_artist_roles.push(DbTrackArtistRole::new(
                &shaped_library::track_id(3_000 + run * 3, 0, 0),
                &shaped_library::artist_id(run),
                9,
                Catalog::MusicBrainz,
                CreditedAs::of(CreditRoleKind::Mixer),
                None,
                format!("{run:08x}-000b-4000-8000-{:012x}", 778),
                chrono::Utc::now(),
            ));
            records.insert_library_rows_for_test(rows).await.unwrap();
        }
    })
    .await;
    // A file staged on this device, and one staged for the cloud, its upload
    // queued.
    let originals = tempfile::tempdir().unwrap();
    let original = originals.path().join("original.bin");
    std::fs::write(&original, vec![0u8; 20_000]).unwrap();
    for (name, place) in [
        ("stage-file-local", coven::Place::DeviceOnly),
        ("stage-file-for-cloud", coven::Place::Cloud),
    ] {
        write_cost(&suffix(name), |run| {
            let records = records.clone();
            let original = original.clone();
            async move {
                let file = NewReleaseFile {
                    id: format!("{run:08x}-0007-4000-8000-{:012x}", name.len()),
                    release_id: shaped_library::release_id(4_000 + run, 0),
                    original_filename: format!("{name}-{run}.flac"),
                    content_type: bae_util::content_type::ContentType::Flac,
                    role: ReleaseFileRole::Other,
                    source_audio: None,
                    created_at: chrono::Utc::now(),
                };
                records.insert_files_placed_for_test(vec![(file, original, place)]).await.unwrap();
            }
        })
        .await;
    }
    // A release queued to download.
    let mut queued = M + 100_000;
    write_cost(&suffix("queue-download"), |run| {
        let db = db.clone();
        let at = queued + run;
        queued += 1;
        async move {
            db.write(move |sql| {
                bae_transfers::tables::bulk::queue_downloads_on(
                    &sql,
                    at,
                    at + 1,
                    &format!("'{}'", shaped_library::release_id(5_001 + (at - M - 100_000), 0)),
                )
            })
            .await
            .unwrap();
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn write_costs() {
    let (db, _copy) = SEEDS.seeded_copy(M).await;
    scenarios(&db, "").await;
    drop(db);
    for (family, drop_family) in FAMILIES {
        let (db, _copy) = SEEDS.seeded_copy_planted(M, drop_family).await;
        scenarios(&db, &format!("/without={family}")).await;
    }
}
