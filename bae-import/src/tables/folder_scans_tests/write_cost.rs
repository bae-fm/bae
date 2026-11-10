//! What a scan's writes cost as its watched folder fills. A measurement, not
//! a check.

use super::*;
use std::time::Instant;

/// The sizes one watched folder is filled to: a library as large as any a
/// person holds reaches the last.
const SIZES: [u64; 3] = [20_000, 100_000, 500_000];

/// The folders between two reports of what a write costs.
const CHUNK: u64 = 2_000;

/// Fill one watched folder through [`SIZES`] a release at a time, as a scan
/// stores each folder it reads, reporting what each [`CHUNK`] of writes took
/// and how long the folder took to reach each size. A write costs the same
/// however many releases the folder holds, so the folder fills in time
/// proportional to its size. Run it in release with
///
/// ```sh
/// scripts/cargo.sh test --release -p bae-import --features test-utils --lib -- \
///     scan_write_cost --ignored --nocapture --test-threads 1
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn scan_write_cost() {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = ImportTables::new(db.clone());
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let started = Instant::now();
    let mut chunk = Instant::now();
    for i in 0..SIZES[SIZES.len() - 1] {
        let name = format!("Artist {:03}/Album {:06}", (i * 7) % 100, (i * 16807) % 999_983);
        tables
            .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate(&root, &name)))
            .await
            .unwrap()
            .unwrap();
        let n = i + 1;
        if n % CHUNK == 0 {
            eprintln!(
                "SCAN_COST n={n} us_per_write={:.1}",
                chunk.elapsed().as_secs_f64() * 1e6 / CHUNK as f64
            );
            chunk = Instant::now();
        }
        if SIZES.contains(&n) {
            eprintln!("SCAN_COST filled n={n} in {:.1} s", started.elapsed().as_secs_f64());
        }
    }
}
