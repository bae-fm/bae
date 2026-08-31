//! The signal-extraction service: one pass over a candidate's files producing a
//! streamed [`Signals`] snapshot (disc ID, barcodes, classified text), consumed
//! by the identify pipeline and the search UI.
//!
//! Emission is streamed so slow OCR doesn't gate the fast signals:
//!
//! 1. **Fast pass.** Everything that resolves without OCR — the disc ID
//!    (LOG/CUE), CUE `CATALOG` barcodes, and the non-OCR text sources
//!    (folder-name brackets, path components, filenames, CUE, text files) — is
//!    gathered up front and emitted as the first `Signals`, so the disc-ID
//!    lookup and the autocomplete populate before the first image OCR finishes.
//! 2. **OCR stream.** Artwork images are analyzed one at a time (a single
//!    `analyze` pass per image yields both barcodes and text). Each image that
//!    adds a barcode or text line re-emits the cumulative `Signals`; the
//!    barcode and text signals settle at the end.
//!
//! A `Release` re-identify resolves its disc ID and artwork from the library.
//! Every snapshot carries the whole `Signals`; the reducer and the UI overwrite
//! wholesale.

use super::analyzer::{ArtworkAnalysis, ArtworkAnalyzer};
use super::cancellation::CancellationRegistry;
use super::candidate_text::{Source, SourcedLine};
use super::fast_pass::{gather_non_ocr_sources, ArtworkImage, FastPass};
use super::pool::Pool;
use super::release::{resolve_release_artwork_paths, resolve_release_identity};
use crate::import::{ImportEvent, ScanEvent};
use crate::library::LibraryManager;
use crate::signals::{
    BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue, TextSignal,
};
use crate::util::rate_limiter::CallPriority;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};

/// Where a candidate's signals come from: a folder on disk, or an existing
/// library release being re-identified.
#[derive(Debug, Clone)]
pub enum ExtractionSource {
    Folder {
        path: PathBuf,
        files: crate::import::folder_scanner::CategorizedFiles,
    },
    Release {
        release_id: String,
    },
}

/// Thread-safe handle to the running signal-extraction service.
#[derive(Clone)]
pub struct ExtractionServiceHandle {
    inner: Arc<ExtractionServiceInner>,
}

struct ExtractionServiceInner {
    runtime_handle: tokio::runtime::Handle,
    event_tx: broadcast::Sender<ImportEvent>,
    /// The platform's artwork analyzer, registered at boot. `None` on a platform
    /// that ships none: artwork is then not a barcode or text source, and
    /// extraction says exactly that (`BarcodeSignal::Absent`) rather than
    /// reporting a decode that never ran.
    analyzer: Mutex<Option<Arc<dyn ArtworkAnalyzer>>>,
    /// Resolves a release's library files for the `Release` re-identify path.
    library_manager: LibraryManager,
    /// Per-candidate cancellation. `start` registers a new entry (cancelling any
    /// prior one for the key); a task releases its own entry on the way out only
    /// when the generation still matches. `ExtractionService::start` also spawns
    /// a bus listener that cancels a key on `ScanEvent::CandidateRemoved`, so a
    /// removed candidate's in-flight OCR stops rather than running to completion.
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
        library_manager: LibraryManager,
    ) -> ExtractionServiceHandle {
        let inner = Arc::new(ExtractionServiceInner {
            runtime_handle,
            event_tx,
            analyzer: Mutex::new(None),
            library_manager,
            cancellation: CancellationRegistry::default(),
        });

        // Subscribe before returning the handle: no extraction can start before
        // this listener is receiving, so a removal naming an in-flight run is
        // never missed.
        let mut removal_rx = inner.event_tx.subscribe();
        let removal_inner = inner.clone();
        inner.runtime_handle.spawn(async move {
            loop {
                match removal_rx.recv().await {
                    Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key })) => {
                        removal_inner.cancellation.cancel(&candidate_key);
                    }
                    Ok(ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate })) => {
                        removal_inner
                            .cancellation
                            .cancel(candidate.path.to_string_lossy().as_ref());
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("signals: candidate-removal listener lagged by {n} import events; an extraction for a removed candidate may run to completion");
                        removal_inner.library_manager.record_telemetry(
                            crate::diagnostics::TelemetryEvent::Anomaly {
                                kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                            },
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        ExtractionServiceHandle { inner }
    }
}

impl ExtractionServiceInner {
    fn has_artwork_analyzer(&self) -> bool {
        self.analyzer.lock().unwrap().is_some()
    }

    async fn analyze_artwork(&self, path: PathBuf) -> Result<ArtworkAnalysis, LookupFailure> {
        let analyzer = self
            .analyzer
            .lock()
            .unwrap()
            .clone()
            .ok_or(LookupFailure::ArtworkAnalysis)?;
        let log_path = path.clone();
        let failure_context = format!("OCR worker failed for {log_path:?}");
        run_blocking(&self.runtime_handle, &failure_context, move || {
            analyzer.analyze(&path)
        })
        .await
        .ok_or(LookupFailure::ArtworkAnalysis)
    }
}

impl ExtractionServiceHandle {
    /// Register the platform's artwork analyzer. Called once at boot from the
    /// bridge's `register_artwork_analyzer`, on the platforms that have one.
    pub fn register_analyzer(&self, analyzer: Arc<dyn ArtworkAnalyzer>) {
        *self.inner.analyzer.lock().unwrap() = Some(analyzer);
    }

    /// Kick off extraction for candidate `key` from `source`. Cancels any prior
    /// in-flight extraction for the same key.
    /// `priority` is the run's, not a call's — extraction makes no provider
    /// calls. It rides the `SignalsUpdated` snapshots so a consumer can tell a
    /// candidate a person opened from one the background sweep picked up.
    pub fn start(&self, key: String, source: ExtractionSource, priority: CallPriority) {
        let inner = self.inner.clone();
        let runtime_handle = self.inner.runtime_handle.clone();
        self.inner
            .cancellation
            .register(key.clone(), move |token, generation| {
                runtime_handle.spawn(async move {
                    run_extraction(inner, key, source, token, generation, priority).await;
                });
            });
    }

    /// Cancel a candidate's in-flight extraction. For the bridge's candidate
    /// teardown (the re-identify dismissal); core-side removal cancels through
    /// the `CandidateRemoved` bus listener instead.
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
    priority: CallPriority,
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
        // One scan derives every non-OCR signal in a single blocking hop, then
        // the artwork OCR streams.
        ExtractionSource::Folder {
            path: folder,
            files,
        } => {
            let fast_folder = folder.clone();
            let Some(fast) = run_fast_pass_blocking(&inner.runtime_handle, move || {
                gather_non_ocr_sources(&fast_folder, &files)
            })
            .await
            else {
                return;
            };
            let mut pool = Pool::default();
            for line in fast.lines {
                pool.push(line);
            }
            for catalog in fast.bracket_catalogs {
                pool.push_bracket(catalog);
            }
            let artwork = ArtworkPass::new(inner.has_artwork_analyzer(), fast.artwork);
            stream_extraction(
                inner,
                key,
                token,
                ExtractionInputs {
                    disc_id: fast.disc_id,
                    barcodes: fast.cue_barcodes,
                    pool,
                    artwork,
                    durations: fast.durations,
                },
                priority,
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
                    // Derived from the library's stored tracks, not a file of a
                    // scanned folder, so there is no row to point at.
                    source_file: None,
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
            // The release's artwork is resolved only when there's an analyzer to
            // decode it with — staging a cover blob nothing will read is pure cost.
            //
            // `_cover_staging` holds the temp dir the cover was staged into and
            // must stay bound until `stream_extraction` returns. A resolve error
            // means the release's files can't be read at all (a missing cover is
            // already a skip inside), so abort rather than emit a misleading
            // settled-with-no-signals result.
            let (artwork, _cover_staging) = match inner.has_artwork_analyzer() {
                true => {
                    match resolve_release_artwork_paths(&inner.library_manager, &release_id).await {
                        // A library release's images are stored blobs, not
                        // files of a scanned folder, so nothing here has a
                        // file id for a signal to point at.
                        Ok((paths, staging)) => (
                            ArtworkPass::new(
                                true,
                                paths
                                    .into_iter()
                                    .map(|path| ArtworkImage {
                                        path,
                                        file_id: None,
                                    })
                                    .collect(),
                            ),
                            staging,
                        ),
                        Err(e) => {
                            error!("signals: cannot read release {release_id} for artwork: {e}; aborting extraction");
                            return;
                        }
                    }
                }
                false => (None, None),
            };
            stream_extraction(
                inner,
                key,
                token,
                ExtractionInputs {
                    disc_id,
                    barcodes: Vec::new(),
                    pool: Pool::default(),
                    artwork,
                    // A library release has no candidate folder to walk, so
                    // nothing is probed on this path.
                    durations: crate::import::probe::SourceDurations::default(),
                },
                priority,
            )
            .await;
        }
    }
}

async fn run_fast_pass_blocking<F>(runtime_handle: &Handle, task: F) -> Option<FastPass>
where
    F: FnOnce() -> Result<FastPass, crate::import::ImportError> + Send + 'static,
{
    match run_blocking(runtime_handle, "fast-pass spawn_blocking failed", task).await {
        Some(Ok(pass)) => Some(pass),
        Some(Err(error)) => {
            error!("signals: folder timing is invalid: {error}; aborting extraction");
            None
        }
        None => None,
    }
}

async fn run_blocking<T, F>(runtime_handle: &Handle, failure_context: &str, task: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    match runtime_handle.spawn_blocking(task).await {
        Ok(value) => Some(value),
        Err(e) => {
            error!("signals: {failure_context}: {e}; aborting extraction");
            None
        }
    }
}

/// What the streaming pass consumes: the settled disc ID, the CUE barcodes, the
/// text pool, and the artwork pass. A folder scan and a release re-identify each
/// build one, differing only in which fields are populated.
struct ExtractionInputs {
    disc_id: DiscIdSignal,
    barcodes: Vec<SourcedValue>,
    pool: Pool,
    artwork: Option<ArtworkPass>,
    durations: crate::import::probe::SourceDurations,
}

/// The artwork OCR pass: the images to decode, and the analyzer that decodes
/// them. `None` in `ExtractionInputs` means artwork is no signal source for this
/// candidate — either it has no images, or the platform has no analyzer. The two
/// are one fact to everything downstream, and pairing the paths with the analyzer
/// makes "images to scan, but nothing to scan them with" unrepresentable.
struct ArtworkPass {
    /// Non-empty by construction.
    images: Vec<ArtworkImage>,
}

impl ArtworkPass {
    fn new(analyzer_available: bool, images: Vec<ArtworkImage>) -> Option<Self> {
        (analyzer_available && !images.is_empty()).then_some(ArtworkPass { images })
    }
}

/// Stream `Signals` over the artwork OCR pass: emit the fast-pass snapshot,
/// then one cumulative snapshot per image that adds a barcode or text line,
/// then a final settled snapshot.
async fn stream_extraction(
    inner: Arc<ExtractionServiceInner>,
    key: String,
    token: CancellationToken,
    inputs: ExtractionInputs,
    priority: CallPriority,
) {
    let ExtractionInputs {
        disc_id,
        mut barcodes,
        mut pool,
        artwork,
        durations,
    } = inputs;
    let has_artwork = artwork.is_some();

    if token.is_cancelled() {
        return;
    }

    // First snapshot: disc ID and CUE barcodes are settled and the autocomplete
    // pool is populated; barcode/text stay `Scanning` while OCR is pending.
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
            durations.clone(),
        ),
        priority,
    );

    // One OCR request at a time (Vision on the ANE is effectively serial).
    if let Some(ArtworkPass { images }) = artwork {
        for ArtworkImage { path, file_id } in images {
            if token.is_cancelled() {
                return;
            }

            let analysis = match inner.analyze_artwork(path.clone()).await {
                Ok(analysis) => analysis,
                Err(failure) => {
                    emit_failed_ocr_signals(
                        &inner, &key, disc_id, &barcodes, &mut pool, failure, durations, priority,
                    );
                    return;
                }
            };

            if token.is_cancelled() {
                return;
            }

            // Accumulate barcodes (deduped by value) and text lines; skip the emit
            // when this image added nothing new.
            let mut changed = false;
            for value in analysis.barcodes {
                // A run of one digit is printed on nothing; OCR reads them off
                // borders and shadows, and looking one up can only miss.
                if crate::signals::is_placeholder_code(&value) {
                    continue;
                }
                if !barcodes.iter().any(|b| b.value == value) {
                    // The image it was read off, so a surface can put the
                    // barcode on that image rather than beside the release.
                    barcodes.push(match &file_id {
                        Some(file_id) => {
                            SourcedValue::in_file(value, SignalOrigin::Artwork, file_id.clone())
                        }
                        None => SourcedValue::new(value, SignalOrigin::Artwork),
                    });
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
                    durations.clone(),
                ),
                priority,
            );
        }
    }

    if token.is_cancelled() {
        return;
    }

    let classification = pool.classify();
    let barcode = if has_artwork || !barcodes.is_empty() {
        BarcodeSignal::Settled { codes: barcodes }
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
                catalogs: classification.catalogs,
                free_text: classification.free_text,
            },
            durations,
        },
        priority,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_failed_ocr_signals(
    inner: &ExtractionServiceInner,
    key: &str,
    disc_id: DiscIdSignal,
    barcodes: &[SourcedValue],
    pool: &mut Pool,
    failure: LookupFailure,
    durations: crate::import::probe::SourceDurations,
    priority: CallPriority,
) {
    let classification = pool.classify();
    let barcode = BarcodeSignal::Failed {
        failure: failure.clone(),
        codes: barcodes.to_vec(),
    };
    emit_signals(
        inner,
        key,
        Signals {
            disc_id,
            barcode,
            text: TextSignal::Failed {
                failure,
                catalogs: classification.catalogs,
                free_text: classification.free_text,
            },
            durations,
        },
        priority,
    );
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
    durations: crate::import::probe::SourceDurations,
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
        durations,
    }
}

/// Send a `Signals` snapshot on the import event bus.
fn emit_signals(
    inner: &ExtractionServiceInner,
    key: &str,
    signals: Signals,
    priority: CallPriority,
) {
    if let Err(err) = inner.event_tx.send(ImportEvent::SignalsUpdated {
        candidate_key: key.to_string(),
        signals,
        priority,
    }) {
        warn!("signals: SignalsUpdated broadcast had no subscribers for {key}: {err}");
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
