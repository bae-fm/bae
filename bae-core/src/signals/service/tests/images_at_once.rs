//! A candidate's images are read several at a time and taken in their order:
//! what the pass sends is the same whatever order the reads finish in, and no
//! more than `IMAGES_IN_FLIGHT` are read at once across every pass.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Condvar;

/// Answers each image only once every image before it in `finish_order` has
/// been answered, so a test fixes the order the reads finish in. The image
/// named `panics_on` panics on its turn instead of answering.
struct OrderedAnalyzer {
    finish_order: Vec<String>,
    panics_on: Option<String>,
    answered: StdMutex<usize>,
    turn_changed: Condvar,
}

impl OrderedAnalyzer {
    fn new(finish_order: Vec<String>) -> Self {
        Self {
            finish_order,
            panics_on: None,
            answered: StdMutex::new(0),
            turn_changed: Condvar::new(),
        }
    }

    fn panicking_on(mut self, name: &str) -> Self {
        self.panics_on = Some(name.to_string());
        self
    }
}

impl ArtworkAnalyzer for OrderedAnalyzer {
    fn analyze(&self, path: &Path) -> ArtworkAnalysis {
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let turn = self
            .finish_order
            .iter()
            .position(|finishes| *finishes == name)
            .expect("every image has a turn");
        let answered = self
            .turn_changed
            .wait_timeout_while(
                self.answered.lock().unwrap(),
                Duration::from_secs(30),
                |answered| *answered < turn,
            )
            .unwrap();
        assert!(!answered.1.timed_out(), "{name} waited for its turn");
        let mut answered = answered.0;
        *answered += 1;
        drop(answered);
        self.turn_changed.notify_all();
        if self.panics_on.as_deref() == Some(name.as_str()) {
            panic!("OCR analyzer panicked on {name}");
        }
        image_analysis(&name)
    }
}

/// What each image reads: a line naming it, and on `p3.jpg` a barcode, so
/// both the text pool and the barcode sightings depend on the order images
/// are taken in.
fn image_analysis(name: &str) -> ArtworkAnalysis {
    ArtworkAnalysis {
        barcodes: if name == "p3.jpg" {
            vec!["5012345678900".to_string()]
        } else {
            Vec::new()
        },
        text_lines: vec![
            format!("Line read off {name}"),
            "Catalog CAT-1001".to_string(),
        ],
    }
}

/// `count` image names, in the order the folder lists them.
fn image_names(count: usize) -> Vec<String> {
    (1..=count).map(|n| format!("p{n}.jpg")).collect()
}

fn folder_with(tmp: &TempDir, images: &[String]) -> PathBuf {
    let names: Vec<&str> = images.iter().map(String::as_str).collect();
    build_release(tmp, "Album Title [XX34b]", &names, &[])
}

/// Every snapshot of one pass over `folder` with `analyzer`, through its end.
async fn pass_snapshots(
    folder: PathBuf,
    analyzer: Arc<dyn ArtworkAnalyzer>,
    expected: usize,
) -> Vec<(Signals, ArtworkScan)> {
    let (_handle, mut rx, mut run, _lib_tmp) = start_signals(folder, analyzer).await;
    let snapshots = collect_snapshots(&mut rx, expected).await;
    run_ended(&mut run).await;
    assert_no_more_snapshots(&mut rx, "the pass's last snapshot");
    snapshots
}

/// Reads that finish last image first send the snapshots, progress and
/// settled signals that reads finishing in the images' order send.
#[tokio::test(flavor = "multi_thread")]
async fn reads_finishing_out_of_order_send_what_reads_in_order_send() {
    let tmp = TempDir::new().unwrap();
    let images = image_names(IMAGES_IN_FLIGHT);
    let folder = folder_with(&tmp, &images);
    // The fast pass, one per image but the last, then the settled snapshot.
    let expected = images.len() + 1;

    let in_order = pass_snapshots(
        folder.clone(),
        Arc::new(OrderedAnalyzer::new(images.clone())),
        expected,
    )
    .await;
    let reversed: Vec<String> = images.iter().rev().cloned().collect();
    let last_first =
        pass_snapshots(folder, Arc::new(OrderedAnalyzer::new(reversed)), expected).await;

    assert_eq!(last_first, in_order);
    let progress: Vec<&ArtworkScan> = in_order.iter().map(|(_, artwork)| artwork).collect();
    let mut reading: Vec<ArtworkScan> = images
        .iter()
        .enumerate()
        .map(|(index, name)| ArtworkScan::Reading {
            current: Some(name.clone()),
            position: index as u32 + 1,
            total: images.len() as u32,
        })
        .collect();
    reading.push(ArtworkScan::Done {
        total: images.len() as u32,
    });
    assert_eq!(progress, reading.iter().collect::<Vec<_>>());
    let settled = &in_order[images.len()].0;
    let artwork_lines: Vec<&str> = settled
        .text_pool
        .iter()
        .filter(|line| line.text.starts_with("Line read off"))
        .map(|line| line.text.as_str())
        .collect();
    let in_image_order: Vec<String> = images
        .iter()
        .map(|name| format!("Line read off {name}"))
        .collect();
    assert_eq!(artwork_lines, in_image_order);
    assert_eq!(
        settled.barcode,
        BarcodeSignal::Settled {
            codes: vec![SourcedValue::in_file(
                "5012345678900".to_string(),
                "p3.jpg".to_string()
            )]
        }
    );
}

/// Counts the reads under way at once, each held until the test opens the
/// gate.
struct ConcurrencyAnalyzer {
    reading: AtomicUsize,
    most_at_once: AtomicUsize,
    calls: AtomicUsize,
    gate: crate::test_gate::Held,
}

impl ArtworkAnalyzer for ConcurrencyAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let reading = self.reading.fetch_add(1, Ordering::SeqCst) + 1;
        self.most_at_once.fetch_max(reading, Ordering::SeqCst);
        self.gate.pass();
        self.reading.fetch_sub(1, Ordering::SeqCst);
        ArtworkAnalysis::empty()
    }
}

/// Wait until `IMAGES_IN_FLIGHT` reads have entered the held analyzer, and
/// check that no other read starts while they are held.
async fn bound_reached_and_held(
    entries: std::sync::mpsc::Receiver<()>,
) -> std::sync::mpsc::Receiver<()> {
    tokio::task::spawn_blocking(move || {
        for _ in 0..IMAGES_IN_FLIGHT {
            entries
                .recv_timeout(Duration::from_secs(30))
                .expect("the bound's worth of images are read at once");
        }
        assert!(
            entries.recv_timeout(Duration::from_millis(200)).is_err(),
            "no image starts while the bound's worth are being read"
        );
        entries
    })
    .await
    .unwrap()
}

fn concurrency_analyzer(gate: crate::test_gate::Held) -> Arc<ConcurrencyAnalyzer> {
    Arc::new(ConcurrencyAnalyzer {
        reading: AtomicUsize::new(0),
        most_at_once: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
        gate,
    })
}

/// `IMAGES_IN_FLIGHT` images are read at once, and no more: while they are
/// all held no other image starts.
#[tokio::test(flavor = "multi_thread")]
async fn no_more_than_the_bound_are_read_at_once() {
    let tmp = TempDir::new().unwrap();
    let images = image_names(IMAGES_IN_FLIGHT * 3);
    let folder = folder_with(&tmp, &images);
    let (gate, held, entries) = crate::test_gate::closed();
    let analyzer = concurrency_analyzer(held);
    let (_handle, mut rx, mut run, _lib_tmp) = start_signals(folder, analyzer.clone()).await;

    let entries = bound_reached_and_held(entries).await;
    gate.open();
    let snapshots = collect_snapshots(&mut rx, images.len() + 1).await;
    run_ended(&mut run).await;
    drop(entries);

    assert_eq!(
        snapshots.last().map(|(_, artwork)| artwork),
        Some(&ArtworkScan::Done {
            total: images.len() as u32
        })
    );
    assert_eq!(analyzer.calls.load(Ordering::SeqCst), images.len());
    assert_eq!(
        analyzer.most_at_once.load(Ordering::SeqCst),
        IMAGES_IN_FLIGHT
    );
}

/// Two passes running at once share the bound: between them no more than
/// `IMAGES_IN_FLIGHT` images are read at once, and both settle.
#[tokio::test(flavor = "multi_thread")]
async fn passes_running_at_once_share_the_bound() {
    let tmp = TempDir::new().unwrap();
    let images = image_names(IMAGES_IN_FLIGHT * 2);
    let names: Vec<&str> = images.iter().map(String::as_str).collect();
    let first = build_release(&tmp, "Album Title A", &names, &[]);
    let second = build_release(&tmp, "Album Title B", &names, &[]);
    let (gate, held, entries) = crate::test_gate::closed();
    let analyzer = concurrency_analyzer(held);
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer.clone());
    let mut first_run = handle.start(
        IdentifyRunId::for_test(1),
        "cand-1".to_string(),
        folder_source(first),
        CallPriority::Interactive,
    );
    let mut second_run = handle.start(
        IdentifyRunId::for_test(2),
        "cand-2".to_string(),
        folder_source(second),
        CallPriority::Interactive,
    );

    let entries = bound_reached_and_held(entries).await;
    gate.open();
    let snapshots = collect_snapshots(&mut rx, 2 * (images.len() + 1)).await;
    run_ended(&mut first_run).await;
    run_ended(&mut second_run).await;
    drop(entries);

    let done = ArtworkScan::Done {
        total: images.len() as u32,
    };
    assert_eq!(
        snapshots
            .iter()
            .filter(|(_, artwork)| *artwork == done)
            .count(),
        2,
        "both passes settle"
    );
    assert_eq!(analyzer.calls.load(Ordering::SeqCst), 2 * images.len());
    assert_eq!(
        analyzer.most_at_once.load(Ordering::SeqCst),
        IMAGES_IN_FLIGHT
    );
}

/// An image whose read dies fails the pass with what the images before it
/// read; the images after it that had already finished add nothing, and
/// nothing follows the failed snapshot.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_read_fails_the_pass_with_the_images_before_it() {
    let tmp = TempDir::new().unwrap();
    let images = image_names(IMAGES_IN_FLIGHT);
    let folder = folder_with(&tmp, &images);
    // p1 finishes, then the images after p2, then p2 panics.
    let finish_order = vec![
        "p1.jpg".to_string(),
        "p4.jpg".to_string(),
        "p3.jpg".to_string(),
        "p2.jpg".to_string(),
    ];
    let analyzer = Arc::new(OrderedAnalyzer::new(finish_order).panicking_on("p2.jpg"));

    // The fast pass, p1's snapshot, then the failed one.
    let snapshots = pass_snapshots(folder, analyzer, 3).await;

    let (after_p1, _) = &snapshots[1];
    let (failed, artwork) = &snapshots[2];
    let ArtworkScan::Failed {
        failure,
        read,
        total,
    } = artwork
    else {
        panic!("the pass fails at p2, got {artwork:?}");
    };
    assert_eq!((*read, *total), (1, images.len() as u32));
    assert!(
        failure.detail.contains("OCR worker failed") && failure.detail.contains("p2.jpg"),
        "the failure names the worker that died and its image, got {failure:?}"
    );
    assert_eq!(
        failed.text,
        TextSignal::Failed {
            failure: failure.clone(),
            catalogs: after_p1.text.catalogs().to_vec(),
            free_text: after_p1.text.free_text().to_vec(),
        }
    );
    assert_eq!(
        failed.barcode,
        BarcodeSignal::Failed {
            failure: failure.clone(),
            codes: Vec::new(),
        },
        "p3's barcode was read after the failure, so it is not a sighting"
    );
    assert_eq!(failed.text_pool, after_p1.text_pool);
    assert!(
        after_p1
            .text_pool
            .iter()
            .any(|line| line.text == "Line read off p1.jpg"),
        "p1's reading is in, got {:?}",
        after_p1.text_pool
    );
}
