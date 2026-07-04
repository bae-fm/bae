//! The signal-extraction service: one pass over a candidate's files producing
//! a streamed [`Signals`] snapshot (disc ID, barcodes, classified text). Both
//! the identify pipeline (which looks the signals up and narrows matches) and
//! the search UI (which surfaces them) consume what this service emits.
//!
//! Emission is streamed so slow OCR doesn't gate the fast signals:
//!
//! 1. **Fast pass.** Everything that resolves without OCR — the disc ID
//!    (LOG/CUE), CUE `CATALOG` barcodes, and the non-OCR text sources
//!    (folder-name brackets, path components, filenames, CUE, text files) — is
//!    gathered up front and emitted as the first `Signals`, so the disc-ID
//!    lookup and the autocomplete populate before the first image OCR
//!    completes.
//! 2. **OCR stream.** Artwork images are analyzed sequentially (one Vision
//!    `analyze` pass per image yields both barcodes and text). Each image that
//!    adds a barcode or text line re-emits the cumulative `Signals`; the
//!    barcode and text signals settle at the end.
//!
//! A `Release` re-identify resolves its disc ID and artwork from the library.
//! Every snapshot carries the whole `Signals`; the reducer and the UI overwrite
//! wholesale.

use super::cancellation::CancellationRegistry;
use super::dump::dump_scan;
use super::fast_pass::{gather_non_ocr_sources, FastPass};
use super::pool::Pool;
use crate::identify::analyzer::{ArtworkAnalyzer, NoopAnalyzer};
use crate::identify::candidate_text::{Source, SourcedLine};
use crate::identify::discid::{resolve_release_artwork_paths, resolve_release_identity};
use crate::import::ImportEvent;
use crate::library::LibraryManager;
use crate::signals::{
    BarcodeSignal, DiscIdSignal, SignalOrigin, Signals, SourcedValue, TextSignal,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

/// Where a candidate's signals come from: a folder on disk, or an existing
/// library release being re-identified.
#[derive(Debug, Clone)]
pub enum ExtractionSource {
    Folder(PathBuf),
    Release { release_id: String },
}

/// Thread-safe handle to the running signal-extraction service.
#[derive(Clone)]
pub struct ExtractionServiceHandle {
    inner: Arc<ExtractionServiceInner>,
}

struct ExtractionServiceInner {
    runtime_handle: tokio::runtime::Handle,
    event_tx: broadcast::Sender<ImportEvent>,
    clock: coven::ClockRef,
    analyzer: Mutex<Arc<dyn ArtworkAnalyzer>>,
    /// Resolves a release's library files for the `Release` re-identify path.
    library_manager: LibraryManager,
    /// Per-candidate cancellation registry. `start` registers a new entry
    /// (cancelling any prior one for the key); tasks release their own entry
    /// on the way out only when its generation still matches.
    cancellation: CancellationRegistry,
}

struct ExtractionRelease {
    inner: Arc<ExtractionServiceInner>,
    key: String,
    generation: u64,
}

impl Drop for ExtractionRelease {
    fn drop(&mut self) {
        self.inner
            .cancellation
            .release_if_current(&self.key, self.generation);
    }
}

/// Builder / entry point for constructing the service.
pub struct ExtractionService;

impl ExtractionService {
    pub fn start(
        runtime_handle: tokio::runtime::Handle,
        event_tx: broadcast::Sender<ImportEvent>,
        clock: coven::ClockRef,
        library_manager: LibraryManager,
    ) -> ExtractionServiceHandle {
        ExtractionServiceHandle {
            inner: Arc::new(ExtractionServiceInner {
                runtime_handle,
                event_tx,
                clock,
                analyzer: Mutex::new(Arc::new(NoopAnalyzer) as Arc<dyn ArtworkAnalyzer>),
                library_manager,
                cancellation: CancellationRegistry::default(),
            }),
        }
    }
}

impl ExtractionServiceHandle {
    /// Share the analyzer with `IdentifyServiceHandle`. Called once at boot
    /// from the bridge's `register_artwork_analyzer`.
    pub fn register_analyzer(&self, analyzer: Arc<dyn ArtworkAnalyzer>) {
        *self.inner.analyzer.lock().unwrap() = analyzer;
    }

    /// Kick off extraction for candidate `key` from `source`. A folder is
    /// scanned for its own sources (artwork, path components, filenames,
    /// CUE, text files); a release re-identify resolves its files from the
    /// library. Cancels any prior in-flight extraction for the same key.
    pub fn start(&self, key: String, source: ExtractionSource) {
        let (token, generation) = self.inner.cancellation.register(key.clone());

        let inner = self.inner.clone();
        self.inner.runtime_handle.spawn(async move {
            run_extraction(inner, key, source, token, generation).await;
        });
    }

    pub fn cancel(&self, key: &str) {
        self.inner.cancellation.cancel(key);
    }
}

/// Drive extraction for one candidate. Builds the inputs for its source, then
/// streams `Signals` snapshots as the disc ID, barcodes, and text settle.
async fn run_extraction(
    inner: Arc<ExtractionServiceInner>,
    key: String,
    source: ExtractionSource,
    token: CancellationToken,
    generation: u64,
) {
    let _release = ExtractionRelease {
        inner: inner.clone(),
        key: key.clone(),
        generation,
    };

    if token.is_cancelled() {
        return;
    }

    match source {
        // Folder: scan once, derive every non-OCR signal in one blocking hop
        // (disc ID, CUE-CATALOG barcodes, text sources), then stream the
        // artwork OCR.
        ExtractionSource::Folder(folder) => {
            let fast_folder = folder.clone();
            let fast =
                match tokio::task::spawn_blocking(move || gather_non_ocr_sources(&fast_folder))
                    .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("signals: fast-pass spawn_blocking failed: {e}");
                        FastPass::empty()
                    }
                };
            let mut pool = Pool::default();
            for line in fast.lines {
                pool.push(line);
            }
            for catalog in fast.bracket_catalogs {
                pool.push_bracket(catalog);
            }
            stream_extraction(
                inner,
                key,
                token,
                ExtractionInputs {
                    disc_id: fast.disc_id,
                    barcodes: fast.cue_barcodes,
                    pool,
                    artwork_paths: fast.artwork_paths,
                    dump_folder: Some(folder),
                },
            )
            .await;
        }

        // Re-identify: disc ID and artwork come from the library, not a folder
        // scan. No non-OCR text sources.
        ExtractionSource::Release { release_id } => {
            let disc_id = match resolve_release_identity(&inner.library_manager, &release_id).await
            {
                Ok((Some(id), track_count)) => DiscIdSignal::Computed {
                    disc_id: id,
                    track_count,
                },
                Ok((None, track_count)) => DiscIdSignal::Absent { track_count },
                Err(detail) => DiscIdSignal::Failed {
                    failure: crate::signals::LookupFailure::Diagnostic { detail },
                    track_count: 0,
                },
            };
            if token.is_cancelled() {
                return;
            }
            // `_cover_staging` holds the temp dir the cover blob was staged into;
            // it must outlive the OCR pass below, so keep it bound until after
            // `stream_extraction` returns. An error here is fatal — the release's
            // files can't be read at all (an optional missing cover is already
            // handled inside as a skip), so the artwork can't be resolved. Fail
            // loud and abort this run rather than masking it as "no artwork" and
            // emitting a misleading settled-with-no-signals result.
            let (artwork_paths, _cover_staging) = match resolve_release_artwork_paths(
                &inner.library_manager,
                &release_id,
            )
            .await
            {
                Ok(staged) => staged,
                Err(e) => {
                    error!("signals: cannot read release {release_id} for artwork: {e}; aborting extraction");
                    return;
                }
            };
            stream_extraction(
                inner,
                key,
                token,
                ExtractionInputs {
                    disc_id,
                    barcodes: Vec::new(),
                    pool: Pool::default(),
                    artwork_paths,
                    dump_folder: None,
                },
            )
            .await;
        }
    }
}

/// Everything a candidate's source yields for the streaming pass to consume:
/// the settled disc ID, the CUE barcodes, the text pool, the artwork images to
/// OCR, and the diagnostic dump folder. A folder scan and a release re-identify
/// each build one of these, differing only in which fields are populated.
struct ExtractionInputs {
    disc_id: DiscIdSignal,
    barcodes: Vec<SourcedValue>,
    pool: Pool,
    artwork_paths: Vec<PathBuf>,
    /// `Some` for folder sources — the diagnostic corpus dump runs after the
    /// final emit; `None` for a release re-identify, which has no folder.
    dump_folder: Option<PathBuf>,
}

/// Stream `Signals` over the artwork OCR pass: emit the fast-pass snapshot,
/// then one cumulative snapshot per image that adds a barcode or text line,
/// then a final settled snapshot.
async fn stream_extraction(
    inner: Arc<ExtractionServiceInner>,
    key: String,
    token: CancellationToken,
    inputs: ExtractionInputs,
) {
    let ExtractionInputs {
        disc_id,
        mut barcodes,
        mut pool,
        artwork_paths,
        dump_folder,
    } = inputs;
    let has_artwork = !artwork_paths.is_empty();

    if token.is_cancelled() {
        return;
    }

    // First snapshot: disc ID and CUE barcodes are already settled and the
    // autocomplete pool is populated. Barcode/text stay `Scanning` while
    // artwork OCR is pending.
    let classification = pool.classify();
    emit_signals(
        &inner,
        &key,
        scanning_signals(
            disc_id.clone(),
            &barcodes,
            has_artwork,
            classification.catalogs,
            classification.free_text,
        ),
    );

    // One OCR request at a time (Vision on the ANE is effectively serial).
    for path in artwork_paths {
        if token.is_cancelled() {
            return;
        }

        let analyzer = inner.analyzer.lock().unwrap().clone();
        let path_clone = path.clone();
        let analysis =
            match tokio::task::spawn_blocking(move || analyzer.analyze(&path_clone)).await {
                Ok(v) => v,
                Err(e) => {
                    warn!("signals: OCR spawn_blocking failed for {path:?}: {e}");
                    continue;
                }
            };

        if token.is_cancelled() {
            return;
        }

        // Accumulate barcodes (deduped by value) and text lines; skip the
        // emit when this image added nothing new. OCR'd codes come from the
        // artwork.
        let mut changed = false;
        for value in analysis.barcodes {
            if !barcodes.iter().any(|b| b.value == value) {
                barcodes.push(SourcedValue::new(value, SignalOrigin::Artwork));
                changed = true;
            }
        }
        let pool_before = pool.lines.len();
        for text in analysis.text_lines {
            pool.push(SourcedLine {
                source: Source::Artwork(path.clone()),
                text,
            });
        }
        if pool.lines.len() != pool_before {
            changed = true;
        }
        if !changed {
            continue;
        }

        // Re-check cancellation before emitting; a successor's `start()` can
        // flip the token during the synchronous push/classify window.
        if token.is_cancelled() {
            return;
        }

        let classification = pool.classify();
        emit_signals(
            &inner,
            &key,
            scanning_signals(
                disc_id.clone(),
                &barcodes,
                has_artwork,
                classification.catalogs,
                classification.free_text,
            ),
        );
    }

    if token.is_cancelled() {
        return;
    }

    // Final settled snapshot.
    let classification = pool.classify();
    let catalogs = classification.catalogs;
    let free_text = classification.free_text;
    let ranked_clusters = classification.ranked_clusters;
    let barcode = if has_artwork || !barcodes.is_empty() {
        BarcodeSignal::Settled {
            codes: barcodes.clone(),
        }
    } else {
        BarcodeSignal::Absent
    };
    emit_signals(
        &inner,
        &key,
        Signals {
            disc_id,
            barcode,
            text: TextSignal::Settled {
                catalogs: catalogs.clone(),
                free_text: free_text.clone(),
            },
        },
    );

    // Diagnostic dump runs after the final emit — the UI already has its
    // final state, so a slow filesystem write doesn't delay completion.
    if let Some(folder) = dump_folder {
        let dump_now = inner.clock.now();
        let dump_key = key;
        let dump_pool = pool;
        tokio::task::spawn_blocking(move || {
            if let Err(e) = dump_scan(
                &dump_key,
                &folder,
                &dump_pool,
                &ranked_clusters,
                &catalogs,
                &free_text,
                dump_now,
            ) {
                debug!("signals: dump failed for {dump_key:?}: {e}");
            }
        });
    }
}

/// Build a `Scanning`-phase `Signals` snapshot. The barcode signal is
/// `Scanning` while artwork OCR is pending; with no artwork it settles
/// immediately (CUE codes only, or `Absent` when there's no source at all).
fn scanning_signals(
    disc_id: DiscIdSignal,
    barcodes: &[SourcedValue],
    has_artwork: bool,
    catalogs: Vec<SourcedValue>,
    free_text: Vec<String>,
) -> Signals {
    let barcode = if has_artwork {
        BarcodeSignal::Scanning {
            codes: barcodes.to_vec(),
        }
    } else if barcodes.is_empty() {
        BarcodeSignal::Absent
    } else {
        BarcodeSignal::Settled {
            codes: barcodes.to_vec(),
        }
    };
    Signals {
        disc_id,
        barcode,
        text: TextSignal::Scanning {
            catalogs,
            free_text,
        },
    }
}

/// Send a `Signals` snapshot on the import event bus.
fn emit_signals(inner: &ExtractionServiceInner, key: &str, signals: Signals) {
    let _ = inner.event_tx.send(ImportEvent::SignalsUpdated {
        candidate_key: key.to_string(),
        signals,
    });
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
