//! What a first scan of a watched folder costs as the folder grows: the whole
//! scan, and what each release folder costs at each point of it, so a cost
//! that grows with what the scan has already stored shows as later folders
//! costing more than early ones. Each release folder holds one small FLAC,
//! hard-linked from one copy, so reading the disk costs the same at every
//! size and the growth left is bae's. Run explicitly, in the release profile:
//!
//!   scripts/cargo.sh test --release -p bae-app --features test-utils \
//!     --test integration -- test_scan_cost:: --ignored --nocapture --test-threads 1
//!
//! `flat_folders` lists every release folder at the top of the watched
//! folder; `one_tree` puts them all below one top-level folder, ten to a
//! subfolder; `flat_folders_with_the_list_open` scans with an import list
//! open and reading its first page, as the import tab does. Each scans 1,000,
//! 5,000, 10,000 and 20,000 folders, or the sizes `SCAN_SIZES` lists
//! (`SCAN_SIZES=5000,20000`).

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use bae_app::AppServices;
use bae_import::{ImportListView, ScanEvent};
use bae_store::ListWindow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SIZES: [usize; 4] = [1_000, 5_000, 10_000, 20_000];

/// The folder counts to scan: `SCAN_SIZES`, comma separated, else [`SIZES`].
fn sizes() -> Vec<usize> {
    match std::env::var("SCAN_SIZES") {
        Ok(text) => {
            text.split(',').map(|n| n.parse().expect("SCAN_SIZES lists folder counts")).collect()
        }
        Err(_) => SIZES.to_vec(),
    }
}

/// How many equal stretches of the scan the per-folder cost is reported over.
const STRETCHES: usize = 10;

/// How the release folders sit under the watched folder.
#[derive(Clone, Copy)]
enum Shape {
    Flat,
    OneTree,
}

impl Shape {
    fn folder(self, root: &Path, n: usize) -> PathBuf {
        match self {
            Shape::Flat => root.join(format!("Album {n}")),
            Shape::OneTree => root
                .join("Collection")
                .join(format!("Artist {}", n / 10))
                .join(format!("Album {n}")),
        }
    }
}

/// `count` one-track release folders under `root`, each track its own name so
/// no two folders hold the same files.
fn write_folders(root: &Path, shape: Shape, count: usize) {
    let source = root.with_extension("source.flac");
    std::fs::copy(bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac"), &source)
        .expect("copy the FLAC fixture");
    for n in 0..count {
        let folder = shape.folder(root, n);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::hard_link(&source, folder.join(format!("01 Track {n}.flac"))).unwrap();
    }
}

/// When each release folder's settled candidate was announced, from the
/// moment the watched folder was added, until the scan finished.
async fn scan(
    services: &AppServices,
    events: &mut bae_import::ScanEvents,
    root: &Path,
) -> (Vec<Duration>, Duration) {
    let start = Instant::now();
    services.import_add_watched_folder(root.to_path_buf()).await.unwrap();
    let mut announced = Vec::new();
    bae_util::test_channels::next_matching(
        events,
        Duration::from_secs(3_600),
        |event| match event {
            ScanEvent::FolderCandidate { .. } => {
                announced.push(start.elapsed());
                None
            }
            ScanEvent::Finished => Some(()),
            _ => None,
        },
    )
    .await
    .expect("the scan finishes");
    (announced, start.elapsed())
}

/// Scan `count` folders laid out as `shape` into a fresh library and print
/// what it cost, optionally with an import list open over the scan.
async fn measure(shape: Shape, count: usize, list_open: bool) {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    write_folders(&root, shape, count);
    let library = temp.path().join("library");
    std::fs::create_dir_all(&library).unwrap();
    let test_library = bae_library::test_libraries::open_test_library(&library).await;
    test_library.manager().set_identify_automatically(false).await.unwrap();
    let services = bae_app::test_services::start_services(&test_library)
        .await
        .expect("the test library's services start");
    let mut events = services.every_scan_event_for_test();

    let list = match list_open {
        false => None,
        true => {
            let list = services.subscribe_import_list(ImportListView::default()).await.unwrap();
            let window = list.until_closed(list.subscribe_window(ListWindow::Forward {
                from: None,
                until: None,
                limit: 50,
            }));
            let reader = tokio::spawn(async move {
                let mut answers = 0_u64;
                while window.next().await.is_ok() {
                    answers += 1;
                }
                answers
            });
            Some((list, reader))
        }
    };

    let (announced, total) = scan(&services, &mut events, &root).await;
    assert_eq!(announced.len(), count, "every folder is announced once");
    let stretch = count / STRETCHES;
    let per_folder: Vec<String> = (0..STRETCHES)
        .map(|index| {
            let from = match index {
                0 => Duration::ZERO,
                index => announced[index * stretch - 1],
            };
            let to = announced[(index + 1) * stretch - 1];
            format!("{:.2}", (to - from).as_secs_f64() * 1_000.0 / stretch as f64)
        })
        .collect();
    let layout = match shape {
        Shape::Flat => "flat",
        Shape::OneTree => "one tree",
    };
    let name =
        format!("{}/{count}", layout.replace(' ', "_") + if list_open { "_list_open" } else { "" });
    bae_store::test_fixtures::window_cost::report_cost(
        &format!("{name}/total"),
        total.as_secs_f64() * 1_000.0,
        "ms",
    );
    // The last tenth of the scan, where a cost that grows with what is stored
    // shows as later folders costing more.
    let last = (announced[count - 1] - announced[count - stretch - 1]).as_secs_f64() * 1_000.0;
    bae_store::test_fixtures::window_cost::report_cost(
        &format!("{name}/last_tenth_per_folder"),
        last / stretch as f64,
        "ms",
    );
    println!(
        "{layout}{} {count} folders: {:.1} s, {:.2} ms a folder; \
         ms a folder by tenth of the scan: {}",
        if list_open { ", list open," } else { "" },
        total.as_secs_f64(),
        total.as_secs_f64() * 1_000.0 / count as f64,
        per_folder.join(" "),
    );
    if let Some((list, reader)) = list {
        list.cancel().await.unwrap();
        println!("  the list answered {} times", reader.await.unwrap());
    }
    services.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "a measurement, run on its own in the release profile"]
async fn flat_folders() {
    for count in sizes() {
        measure(Shape::Flat, count, false).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "a measurement, run on its own in the release profile"]
async fn one_tree() {
    for count in sizes() {
        measure(Shape::OneTree, count, false).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "a measurement, run on its own in the release profile"]
async fn flat_folders_with_the_list_open() {
    for count in sizes() {
        measure(Shape::Flat, count, true).await;
    }
}
