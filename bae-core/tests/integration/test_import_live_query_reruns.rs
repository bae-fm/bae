//! How often the import screen's live queries rerun while a scan writes
//! folders, with the screen open on one candidate: the pane's read of that
//! candidate, the list, and the scans' progress. The pane reads only its
//! candidate's rows, so a scan writing other folders must leave it alone; the
//! list and the progress show every folder and rerun with each write. Each
//! folder holds one file of a size no other folder's file has, so each write
//! stores rows of its own, as a real scan's folders do. Run explicitly:
//!
//!   scripts/cargo.sh test -p bae-core --features test-utils --test integration \
//!     -- test_import_live_query_reruns:: --ignored --nocapture
//!
//! It prints each query's runs over the writes: all of them, those a write
//! caused, and of those, the ones whose result was unchanged.

use bae_core::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, ScanItem, ScannedAudio, ScannedFile,
};
use bae_core::import::{
    FolderCandidate, ImportListOrder, ImportListView, ReleaseFileScope, TriageTab,
};
use bae_core::library::{AppServices, LibraryPageWindow};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// The scan writes, one folder each, after the candidate the pane shows.
const WRITES: u64 = 1_000;

/// How long one of the screen's reads may take to show a write.
const SETTLE: Duration = Duration::from_secs(60);

/// The folder `name` below `root`, holding one FLAC of `size` bytes.
fn folder(root: &str, name: &str, size: u64) -> ScanItem {
    let path = Path::new(root).join(name);
    let mut file = ScannedFile::new(path.join("01.flac"), "01.flac".to_string(), size, 1);
    file.source_audio = Some(ScannedAudio {
        content_type: bae_util::content_type::ContentType::Flac,
        duration_ms: 5_000,
        format: bae_core::album_detail::AudioFormat {
            codec: "FLAC".to_string(),
            sample_rate_hz: 44_100,
            bits_per_sample: Some(16),
            bitrate_kbps: None,
            channels: 2,
        },
    });
    ScanItem::Valid(FolderCandidate {
        file_root: path.clone(),
        path,
        name: name.to_string(),
        files: CategorizedFiles {
            files: vec![CandidateFile {
                file,
                role: FileRole::Audio,
            }],
            parts: Vec::new(),
        },
        watched_folder_path: root.to_string(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: name.to_string(),
        grouping: None,
    })
}

/// What a run counter has counted so far.
#[derive(Clone, Copy)]
struct Counted {
    runs: u64,
    writes: u64,
    equal: u64,
}

impl Counted {
    fn of(runs: &coven::LiveQueryRuns) -> Self {
        Self {
            runs: runs.runs(),
            writes: runs.writes(),
            equal: runs.equal(),
        }
    }

    fn since(self, before: Self) -> Self {
        Self {
            runs: self.runs - before.runs,
            writes: self.writes - before.writes,
            equal: self.equal - before.equal,
        }
    }
}

/// Wait until `seen` holds a value `ready` accepts, failing after [`SETTLE`].
async fn settle<T>(
    seen: &mut tokio::sync::watch::Receiver<T>,
    what: &str,
    ready: impl FnMut(&T) -> bool,
) {
    tokio::time::timeout(SETTLE, seen.wait_for(ready))
        .await
        .unwrap_or_else(|_| panic!("{what} did not show the write within {SETTLE:?}"))
        .unwrap_or_else(|_| panic!("{what} ended"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a measurement of live query reruns, run on its own"]
async fn a_scan_leaves_the_open_candidate_pane_alone() {
    let temp = tempfile::TempDir::new().unwrap();
    let library = temp.path().join("library");
    std::fs::create_dir_all(&library).unwrap();
    let (manager, db) = bae_test_support::open_test_library(&library).await;
    manager.set_identify_automatically(false).await.unwrap();
    let services = AppServices::for_test(manager).await.unwrap();

    let root = temp.path().join("watched").to_string_lossy().into_owned();
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db
        .begin_folder_scan(&root, bae_core::import::VolumeKind::Local)
        .await
        .unwrap();
    let seed = folder(&root, "0000 seed", 1_000);
    let ScanItem::Valid(seed_candidate) = &seed else {
        unreachable!("the folder is valid")
    };
    let key = seed_candidate.path.to_string_lossy().into_owned();
    db.save_folder_scan_item(&root, generation, &seed)
        .await
        .unwrap();

    // The screen: the list's first page in path order, the scans' progress,
    // and the pane on the seed, each read as the app reads it.
    let list = Arc::new(
        services
            .subscribe_import_list(ImportListView {
                tab: TriageTab::Pending,
                order: ImportListOrder::PathAscending,
                ..ImportListView::default()
            })
            .await
            .unwrap(),
    );
    list.set_windows(
        std::iter::once(LibraryPageWindow {
            offset: 0,
            limit: 50,
        })
        .collect(),
    )
    .unwrap();
    let list_runs = list.runs_for_test();
    let (list_tx, mut list_seen) = tokio::sync::watch::channel(0_u64);
    let reading = list.clone();
    tokio::spawn(async move {
        while let Ok(snapshot) = reading.next().await {
            list_tx.send_replace(snapshot.total_count);
        }
    });

    let mut progress = services.subscribe_folder_scans();
    let progress_runs = progress.runs();
    let (progress_tx, mut progress_seen) = tokio::sync::watch::channel(0_u64);
    let progress_reader = tokio::spawn(async move {
        while let Some(read) = progress.next().await {
            let found = read
                .expect("the progress reads")
                .activity
                .expect("the scan is running")
                .found_count;
            progress_tx.send_replace(found);
        }
    });

    let (pane, pane_runs) = services.subscribe_import_candidate_with_runs_for_test();
    let pane = Arc::new(pane);
    pane.set(Some(key.clone())).unwrap();
    let (pane_tx, mut pane_seen) = tokio::sync::watch::channel(None::<bool>);
    let reading = pane.clone();
    tokio::spawn(async move {
        while let Ok(snapshot) = reading.next().await {
            if let Some(detail) = snapshot.value {
                pane_tx.send_replace(Some(detail.skipped));
            }
        }
    });

    settle(&mut list_seen, "the list", |total| *total == 1).await;
    settle(&mut progress_seen, "the progress", |found| *found == 1).await;
    settle(&mut pane_seen, "the pane", |skipped| {
        *skipped == Some(false)
    })
    .await;
    let list_before = Counted::of(&list_runs);
    let progress_before = Counted::of(&progress_runs);
    let pane_before = Counted::of(&pane_runs);

    for n in 1..=WRITES {
        db.save_folder_scan_item(
            &root,
            generation,
            &folder(&root, &format!("{n:05} folder"), 2_000 + n),
        )
        .await
        .unwrap();
    }
    settle(&mut list_seen, "the list", |total| *total == WRITES + 1).await;
    settle(&mut progress_seen, "the progress", |found| {
        *found == WRITES + 1
    })
    .await;
    let list_counted = Counted::of(&list_runs).since(list_before);
    let progress_counted = Counted::of(&progress_runs).since(progress_before);

    // The pane has nothing to show for the scan's writes, so a write it does
    // show closes them: once the pane shows it, it has run for everything
    // the scan wrote before it. Its runs, less the one that showed that
    // write, are the scan's.
    db.set_import_candidate_skipped(&key, true).await.unwrap();
    settle(&mut pane_seen, "the pane", |skipped| *skipped == Some(true)).await;
    let pane_counted = Counted::of(&pane_runs).since(pane_before);
    let pane_counted = Counted {
        runs: pane_counted.runs - 1,
        writes: pane_counted.writes - 1,
        equal: pane_counted.equal,
    };

    println!("{WRITES} scan writes, runs (caused by a write, of those unchanged):");
    for (name, counted) in [
        ("candidate pane", pane_counted),
        ("list", list_counted),
        ("progress", progress_counted),
    ] {
        println!(
            "  {name:14} {:5} ({:5}, {:5})",
            counted.runs, counted.writes, counted.equal
        );
    }

    // Coven reruns a query for a commit only when the commit changes a row
    // the query's conditions allow, which depends on the rows written, not on
    // timing, and no scan write here writes a row of the pane's candidate: the
    // count is 0 on every run. A read coven can't keep to that candidate's
    // rows passes every write and reruns for nearly all of them, so there is
    // nothing in between for a looser bound to allow.
    assert_eq!(
        pane_counted.runs, 0,
        "the scan's writes of other folders reran the candidate pane"
    );

    list.cancel().await.unwrap();
    pane.cancel();
    // The progress query ends with the store, so it goes first.
    progress_reader.abort();
    assert!(progress_reader.await.unwrap_err().is_cancelled());
    services.close().await.unwrap();
}
