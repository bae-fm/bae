//! How often the import screen's live queries rerun while a scan writes
//! folders, with the screen open on one candidate: the pane's read of that
//! candidate, the list's count and two of its windows, and the scans'
//! progress; and, in a test of its own, the read of that candidate's cover
//! picker. The pane and the picker read only their candidate's rows, so a scan
//! writing other folders must not rerun their queries; the count and the
//! progress show every folder and rerun with each write, and a window reruns
//! for the writes it can hold. Each folder holds one file of a size no other
//! folder's file has, so each write stores rows of its own, as a real scan's
//! folders do. Run explicitly:
//!
//!   scripts/cargo.sh test -p bae-app --features test-utils --test integration \
//!     -- test_import_live_query_reruns:: --ignored --nocapture
//!
//! It prints each query's runs over the writes: all of them, those a write
//! caused, and of those, the ones whose result was unchanged.

use bae_import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, ScanItem, ScannedAudio, ScannedFile,
};
use bae_import::RelativeFolder;
use bae_import::{
    CandidateKey, FolderCandidate, ImportListOrder, ImportListView, ReleaseFileScope, TriageTab,
};
use bae_store::ListWindow;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// The scan writes, one folder each, after the candidate the pane shows.
const WRITES: u64 = 1_000;

/// How long one of the screen's reads may take to show a write.
const SETTLE: Duration = Duration::from_secs(60);

/// The folder `name` below `root`, holding one FLAC of `size` bytes.
fn folder(root: &Path, name: &str, size: u64) -> ScanItem {
    folder_with(root, name, size, &[])
}

/// The folder `name` below `root`, holding one FLAC of `size` bytes and the
/// images `images`, by their names.
fn folder_with(root: &Path, name: &str, size: u64, images: &[&str]) -> ScanItem {
    let path = root.join(name);
    let mut file = ScannedFile::new(path.join("01.flac"), "01.flac".to_string(), size, 1);
    file.source_audio = Some(ScannedAudio {
        content_type: bae_util::content_type::ContentType::Flac,
        container: bae_util::audio_container::AudioContainer::Flac,
        duration_ms: 5_000,
        format: bae_types::audio_format::AudioFormat {
            codec: "FLAC".to_string(),
            sample_rate_hz: 44_100,
            bits_per_sample: Some(16),
            bitrate_kbps: None,
            channels: 2,
        },
    });
    let images = images.iter().map(|image| CandidateFile {
        file: ScannedFile::new(path.join(image), image.to_string(), 500, 1),
        role: FileRole::Artwork,
    });
    ScanItem::Valid(FolderCandidate {
        file_root: path.clone(),
        path: path.clone(),
        name: name.to_string(),
        files: CategorizedFiles {
            files: std::iter::once(CandidateFile { file, role: FileRole::Audio })
                .chain(images)
                .collect(),
            parts: Vec::new(),
        },
        watched_folder_path: root.to_path_buf(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        relative_folder: RelativeFolder::of(Path::new(name)).unwrap(),
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
        Self { runs: runs.runs(), writes: runs.writes(), equal: runs.equal() }
    }

    fn since(self, before: Self) -> Self {
        Self {
            runs: self.runs - before.runs,
            writes: self.writes - before.writes,
            equal: self.equal - before.equal,
        }
    }

    /// The runs a write counted here caused before the closing write: a
    /// write the reader shows once it has run for every write before it,
    /// which caused one run whose result changed.
    fn without_closing_write(self) -> Self {
        Self { runs: self.runs - 1, writes: self.writes - 1, equal: self.equal }
    }

    /// Print each query's runs over the scan's writes, by its name, and
    /// report its runs and those a write caused as costs.
    fn report(measured: &[(&str, Self)]) {
        println!("{WRITES} scan writes, runs (caused by a write, of those unchanged):");
        for (name, counted) in measured {
            println!("  {name:14} {:5} ({:5}, {:5})", counted.runs, counted.writes, counted.equal);
            let metric = name.replace(' ', "_");
            for (kind, count) in [("runs", counted.runs), ("caused_by_a_write", counted.writes)] {
                bae_store::test_fixtures::window_cost::report_cost(
                    &format!("{metric}/{kind}"),
                    count as f64,
                    "count",
                );
            }
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
    let temp = bae_util::test_files::TestFolder::new();
    let library = temp.create_folder("library");
    let test_library = bae_library::test_libraries::open_test_library(&library).await;
    let tables = bae_import::tables::ImportTables::new(test_library.store().clone());
    test_library.manager().set_identify_automatically(false).await.unwrap();
    let services = bae_app::test_services::start_services(&test_library)
        .await
        .expect("the test library's services start");

    let root = temp.path().join("watched");
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, bae_import::VolumeKind::Local).await.unwrap();
    let seed = folder(&root, "0000 seed", 1_000);
    let ScanItem::Valid(seed_candidate) = &seed else { unreachable!("the folder is valid") };
    let key = CandidateKey::folder(&seed_candidate.path);
    tables.save_folder_scan_item(&root, generation, &seed).await.unwrap();

    // The screen: the list in path order, as its count and two windows of
    // its own: the seed alone, which the scan's folders sort after, and the
    // one from the seed on, which holds the scan's folders; the scans'
    // progress, and the pane on the seed, each read as the app reads it.
    let list = bae_import::ImportListRequest::of(ImportListView {
        tab: TriageTab::Pending,
        order: ImportListOrder::PathAscending,
        ..ImportListView::default()
    });
    let seed_key = services
        .import_list_key_at_for_test(list.clone(), 0)
        .await
        .unwrap()
        .expect("the seed is listed");
    let mut count = services.import_list_count_for_test(list.clone());
    let (count_tx, mut list_seen) = tokio::sync::watch::channel(None::<(u64, Counted)>);
    let count_reader = bae_util::test_spawn::task(async move {
        while let Some(read) = count.next_inline().await {
            let shown = read.expect("the count reads");
            count_tx.send_replace(Some((shown.rows, Counted::of(&count.runs()))));
        }
    });
    let mut windows = Vec::new();
    for (name, window) in [
        ("window above the scan", ListWindow::top(1)),
        ("window that holds the scan", ListWindow { from: Some(seed_key.clone()), limit: 50 }),
    ] {
        let window = services.import_list_window_for_test(list.clone(), window);
        let (tx, seen) = tokio::sync::watch::channel(0_u64);
        let window = Arc::new(window);
        let reading = window.clone();
        let reader = bae_util::test_spawn::task(async move {
            while let Ok(event) = reading.next().await {
                let delivered = event.result.expect("the window reads").rows.len();
                tx.send_replace(delivered as u64);
            }
        });
        windows.push((name, window, seen, reader));
    }

    // The scanning folders with each found count, beside how often the query
    // had run when it returned it. The test writes this scan straight to the
    // tables, so it holds the root as running in place of a coordinator.
    tables.hold_root_running_for_test(&root).await.unwrap();
    let mut progress = tables.subscribe_scanning_folders();
    let (progress_tx, mut progress_seen) = tokio::sync::watch::channel(None::<(u64, Counted)>);
    let progress_reader = bae_util::test_spawn::task(async move {
        while let Some(read) = progress.next_inline().await {
            let folders = read.expect("the scanning folders read");
            let [bae_import::ScanningFolder { found, .. }] = folders[..] else {
                panic!("one scan is running: {folders:?}");
            };
            progress_tx.send_replace(Some((found, Counted::of(&progress.runs()))));
        }
    });

    // The pane's query, read as the pane reads it, each value beside how
    // often the query had run when it returned it.
    let mut pane = services.subscribe_import_candidate_query_for_test();
    pane.requests().set(Some(key.clone())).unwrap();
    let (pane_tx, mut pane_seen) = tokio::sync::watch::channel(None::<(bool, Counted)>);
    let pane_reader = bae_util::test_spawn::task(async move {
        while let Some(event) = pane.next_inline().await {
            if let Some(projection) = event.result.expect("the pane reads") {
                let skipped = projection.resolve().skipped;
                pane_tx.send_replace(Some((skipped, Counted::of(&pane.runs()))));
            }
        }
    });

    settle(&mut list_seen, "the count", |seen| matches!(seen, Some((1, _)))).await;
    settle(&mut progress_seen, "the progress", |seen| matches!(seen, Some((1, _)))).await;
    settle(&mut pane_seen, "the pane", |shown| matches!(shown, Some((false, _)))).await;
    let window_runs = || -> Vec<Counted> {
        windows.iter().map(|(_, window, _, _)| Counted::of(&window.runs())).collect()
    };
    let windows_before = window_runs();
    let count_before = list_seen.borrow().expect("the count showed the seed").1;
    let progress_before = progress_seen.borrow().expect("the progress showed the seed").1;
    let pane_before = pane_seen.borrow().expect("the pane showed the candidate").1;

    for n in 1..=WRITES {
        tables
            .save_folder_scan_item(
                &root,
                generation,
                &folder(&root, &format!("{n:05} folder"), 2_000 + n),
            )
            .await
            .unwrap();
    }
    settle(
        &mut list_seen,
        "the count",
        |seen| matches!(seen, Some((rows, _)) if *rows == WRITES + 1),
    )
    .await;
    settle(
        &mut progress_seen,
        "the progress",
        |seen| matches!(seen, Some((found, _)) if *found == WRITES + 1),
    )
    .await;
    let count_counted =
        list_seen.borrow().expect("the count showed the writes").1.since(count_before);
    let windows_counted: Vec<Counted> = window_runs()
        .into_iter()
        .zip(windows_before)
        .map(|(after, before)| after.since(before))
        .collect();
    let progress_counted =
        progress_seen.borrow().expect("the progress showed the writes").1.since(progress_before);

    // The pane has nothing to show for the scan's writes, so a write it does
    // show closes them: once the pane shows it, it has run for everything
    // the scan wrote before it. Its runs, less the one that showed that
    // write, are the scan's.
    tables.set_import_candidate_skipped(&seed_candidate.path, true).await.unwrap();
    settle(&mut pane_seen, "the pane", |shown| matches!(shown, Some((true, _)))).await;
    let pane_counted = pane_seen
        .borrow()
        .expect("the pane showed the candidate")
        .1
        .since(pane_before)
        .without_closing_write();

    Counted::report(&[
        ("candidate pane", pane_counted),
        ("count", count_counted),
        ("window above the scan", windows_counted[0]),
        ("window that holds the scan", windows_counted[1]),
        ("progress", progress_counted),
    ]);

    // Coven reruns a query for a commit only when the commit changes a row
    // the query's conditions allow, which depends on the rows written, not on
    // timing, and no scan write here writes a row of the pane's candidate: the
    // count is 0 on every run. A read coven can't keep to that candidate's
    // rows passes every write and reruns for nearly all of them, so there is
    // nothing in between for a looser bound to allow.
    assert_eq!(pane_counted.runs, 0, "the scan's writes of other folders reran the candidate pane");

    // Each of these readers fails on a read that reports its query ended, as
    // the pane's, the progress's and the count's do once the store closes, so
    // they stop first.
    for reader in [pane_reader, progress_reader, count_reader] {
        reader.abort();
        assert!(reader.await.unwrap_err().is_cancelled());
    }
    services.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a measurement of live query reruns, run on its own"]
async fn a_scan_leaves_the_open_cover_gallery_alone() {
    let temp = tempfile::TempDir::new().unwrap();
    let library = temp.path().join("library");
    std::fs::create_dir_all(&library).unwrap();
    let test_library = bae_library::test_libraries::open_test_library(&library).await;
    let tables = bae_import::tables::ImportTables::new(test_library.store().clone());
    test_library.manager().set_identify_automatically(false).await.unwrap();
    let services = bae_app::test_services::start_services(&test_library)
        .await
        .expect("the test library's services start");

    let root = temp.path().join("watched");
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, bae_import::VolumeKind::Local).await.unwrap();
    let seed = folder(&root, "0000 seed", 1_000);
    let ScanItem::Valid(seed_candidate) = &seed else { unreachable!("the folder is valid") };
    let key = CandidateKey::folder(&seed_candidate.path);
    tables.save_folder_scan_item(&root, generation, &seed).await.unwrap();

    // The picker's query on the seed under all its sources, read as the
    // picker reads it, each count of the folder's images beside how often
    // the query had run when it returned it.
    let mut gallery = tables.subscribe_cover_gallery(Some(bae_import::CoverGalleryRequest {
        owner: bae_import::CoverGalleryOwner::Candidate(key),
        filter: bae_import::CoverFilter::All,
        focus: None,
    }));
    let (gallery_tx, mut gallery_seen) = tokio::sync::watch::channel(None::<(u32, Counted)>);
    let gallery_reader = bae_util::test_spawn::task(async move {
        while let Some(event) = gallery.next_inline().await {
            if let Some(snapshot) = event.result.expect("the picker reads") {
                let images = snapshot.gallery.files.expect("all shows the folder's images").count;
                gallery_tx.send_replace(Some((images, Counted::of(&gallery.runs()))));
            }
        }
    });
    settle(&mut gallery_seen, "the picker", |shown| matches!(shown, Some((0, _)))).await;
    let gallery_before = gallery_seen.borrow().expect("the picker showed the seed").1;

    for n in 1..=WRITES {
        tables
            .save_folder_scan_item(
                &root,
                generation,
                &folder(&root, &format!("{n:05} folder"), 2_000 + n),
            )
            .await
            .unwrap();
    }

    // The picker has nothing to show for the scan's writes, so a write it
    // does show closes them: the scan finds an image in the seed's folder,
    // and once the picker shows it, it has run for everything the scan wrote
    // before it. Its runs, less the one that showed that write, are the
    // scan's.
    tables
        .save_folder_scan_item(
            &root,
            generation,
            &folder_with(&root, "0000 seed", 1_000, &["cover.jpg"]),
        )
        .await
        .unwrap();
    settle(&mut gallery_seen, "the picker", |shown| matches!(shown, Some((1, _)))).await;
    let gallery_counted = gallery_seen
        .borrow()
        .expect("the picker showed the image")
        .1
        .since(gallery_before)
        .without_closing_write();

    Counted::report(&[("cover gallery", gallery_counted)]);

    // As for the pane: no scan write here writes a row of the picker's
    // candidate, so the count is 0 on every run.
    assert_eq!(
        gallery_counted.runs, 0,
        "the scan's writes of other folders reran the cover gallery"
    );

    gallery_reader.abort();
    assert!(gallery_reader.await.unwrap_err().is_cancelled());
    services.close().await.unwrap();
}
