//! What one window of the import list costs to read, at the top, the middle
//! and the end of the list, at the sizes db's list windows are measured at,
//! reached by a jump and by scrolling through the list's live query. A
//! measurement, not a check: run it in release with
//!
//! ```sh
//! scripts/cargo.sh test --release -p bae-core --features test-utils --lib -- \
//!     import_list_window_cost --ignored --nocapture --test-threads 1
//! ```

use super::super::*;
use crate::db::client::tests::window_cost::{offsets, report, LIMIT, RUNS, SIZES};
use std::time::{Duration, Instant};

/// Scan `start..end` more folders into the import list, each its own album
/// folder under one of 100 artist folders, in a watched folder of their own.
pub(crate) async fn grow_import_list(
    db: &Database,
    tmp: &std::path::Path,
    start: u64,
    end: u64,
) -> Duration {
    let tables = crate::import::tables::ImportTables::new(db.clone());
    let root = tmp.join(format!("watched-{start}"));
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(&root).unwrap().to_str().unwrap().to_string();
    let started = Instant::now();
    tables.add_watched_import_folder(std::path::Path::new(&root)).await.unwrap();
    let generation = tables
        .begin_folder_scan(std::path::Path::new(&root), crate::import::VolumeKind::Local)
        .await
        .unwrap();
    for i in start..end {
        let name = format!("Artist {:03}/Album {:06}", (i * 7) % 100, (i * 16807) % 999_983);
        let candidate = super::candidate(std::path::Path::new(&root), &name);
        tables
            .save_folder_scan_item(
                std::path::Path::new(&root),
                generation,
                &crate::import::folder_scanner::ScanItem::Valid(candidate),
            )
            .await
            .unwrap()
            .unwrap();
    }
    tables
        .finish_folder_scan(std::path::Path::new(&root), generation, None)
        .await
        .unwrap()
        .unwrap();
    started.elapsed()
}

/// One read of the import list's Found tab with one window, at each measured
/// offset, in each order, through the list's live query: the counts, the
/// window's items and their rows. A jump moves the window between the offset
/// and one far from it, so no read knows a row beside its window; a scroll
/// moves it between the offset and the window after it.
async fn measure_import_list(db: &Database, n: u64) {
    let tables = crate::import::tables::ImportTables::new(db.clone());
    use crate::import::{ImportListOrder, ImportListRequest, ImportListView, PendingFilter};
    for order in [ImportListOrder::PathAscending, ImportListOrder::NewestFirst] {
        let list = tables
            .open_import_list(ImportListView {
                tab: crate::import::TriageTab::Pending,
                filter_text: String::new(),
                pending_filter: PendingFilter::All,
                order,
            })
            .await
            .unwrap();
        let request = |offset| ImportListRequest {
            list,
            windows: crate::library::LibraryPageWindows::from([
                crate::library::LibraryPageWindow { offset, limit: LIMIT },
            ]),
        };
        for (at, offset) in offsets(n) {
            let away = if offset >= n / 2 { offset - n / 4 } else { offset + n / 4 };
            let next = if offset + 2 * LIMIT <= n { offset + LIMIT } else { offset - LIMIT };
            for (reach, other) in [("jump", away), ("scroll", next)] {
                let mut query = tables.subscribe_import_list(request(other));
                query.next().await.unwrap().result.unwrap();
                let requests = query.requests();
                let mut runs = Vec::with_capacity(RUNS);
                let mut rows = 0;
                for run in 0..2 * RUNS {
                    let target = if run % 2 == 0 { offset } else { other };
                    let started = Instant::now();
                    requests.set(request(target)).unwrap();
                    let read = query.next().await.unwrap().result.unwrap();
                    if target == offset {
                        runs.push(started.elapsed());
                        rows = read.windows[0].items.len();
                    }
                }
                runs.sort();
                report(
                    "import_list",
                    &format!("{order:?}"),
                    reach,
                    at,
                    offset,
                    n,
                    (runs[RUNS / 2], rows),
                );
            }
        }
        tables.close_import_list(list).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn import_list_window_cost() {
    let (db, tmp) = super::empty_db().await;
    let mut held = 0;
    for n in SIZES {
        let took = grow_import_list(&db, tmp.path(), held, n).await;
        eprintln!("WINDOW_COST scanned n={n} in {:.1} s", took.as_secs_f64());
        held = n;
        measure_import_list(&db, n).await;
    }
}
