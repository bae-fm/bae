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
//!    `analyze` pass per image yields both barcodes and text). Every image
//!    read re-emits the cumulative `Signals` with the pass's position moved
//!    on, so a surface can show which image is being read; the barcode and
//!    text signals settle at the end.
//!
//! A `Release` re-identify resolves its disc ID and artwork from the library.
//! Every snapshot carries the whole `Signals`; the reducer and the UI overwrite
//! wholesale.
//!
//! Every snapshot goes two ways. The run the extraction feeds reads it off a
//! [`ExtractionWatch`] handed out at `start`, which holds the latest snapshot
//! and nothing else: however far behind the run looks, it sees what its own
//! extraction last said. The bus carries the same snapshot, named for the
//! run, to everything that watches candidates rather than drives one — the
//! sweep pass and the explicit-lookup recorder storing a verdict beside its
//! snapshot, the candidate runtime, the UI.
//!
//! A snapshot goes out only while its extraction is the key's current one.
//! Starting a run replaces the extraction behind the previous run of the same
//! candidate, and that one may still be mid-pass; nothing it has left to say
//! reaches the bus or its watch.

use super::analyzer::{ArtworkAnalysis, ArtworkAnalyzer};
use super::cancellation::CancellationRegistry;
use super::candidate_text::{Source, SourcedLine};
use super::fast_pass::{gather_non_ocr_sources, ArtworkImage, FastPass};
use super::pool::Pool;
use super::release::{resolve_release_artwork_paths, resolve_release_identity};
use crate::identify::IdentifyRunId;
use crate::import::{ImportEvent, ScanEvent};
use crate::library::LibraryManager;
use crate::signals::{
    ArtworkScan, BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue,
    TextSignal,
};
use crate::util::rate_limiter::CallPriority;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

/// Where a candidate's signals come from: a folder on disk, or an existing
/// library release being re-identified.
#[derive(Debug, Clone)]
pub enum ExtractionSource {
    Candidate {
        candidate: crate::import::release_candidate::ReleaseCandidate,
    },
    Release {
        release_id: String,
    },
}

/// One snapshot of a candidate's signals, with where the artwork pass that
/// produced it had got to.
#[derive(Debug, Clone)]
pub struct SignalsSnapshot {
    pub signals: Signals,
    pub artwork: ArtworkScan,
}

/// A run's view of the extraction feeding it: the latest snapshot, replaced
/// wholesale as the pass goes, `None` until the first. A watch rather than the
/// bus, so the run reads what its extraction last said however late it looks,
/// and never what another extraction said. The sender goes with the
/// extraction, so a run can also tell that its extraction is over.
pub type ExtractionWatch = watch::Receiver<Option<SignalsSnapshot>>;

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

/// One extraction in flight: the run it feeds, the candidate, the registry
/// generation that says whether it is still the current one, the run's
/// priority, and the watch the run reads its snapshots off.
struct RunningExtraction {
    run: IdentifyRunId,
    key: String,
    generation: u64,
    priority: CallPriority,
    snapshots: watch::Sender<Option<SignalsSnapshot>>,
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

    /// Kick off extraction for candidate `key` from `source`, feeding `run`,
    /// and hand back the watch the run reads its snapshots off. Cancels any
    /// prior in-flight extraction for the same key, and from here on that one
    /// emits nothing: its snapshots would name a run that is over.
    /// `priority` is the run's, not a call's — extraction makes no provider
    /// calls. It rides the `SignalsUpdated` snapshots so a consumer can tell a
    /// candidate a person opened from one the background sweep picked up.
    pub fn start(
        &self,
        run: IdentifyRunId,
        key: String,
        source: ExtractionSource,
        priority: CallPriority,
    ) -> ExtractionWatch {
        let inner = self.inner.clone();
        let runtime_handle = self.inner.runtime_handle.clone();
        let (snapshots, watch) = watch::channel(None);
        self.inner
            .cancellation
            .register(key.clone(), move |token, generation| {
                let extraction = RunningExtraction {
                    run,
                    key,
                    generation,
                    priority,
                    snapshots,
                };
                runtime_handle.spawn(async move {
                    run_extraction(inner, extraction, source, token).await;
                });
            });
        watch
    }

    /// Cancel a candidate's in-flight extraction. For the bridge's candidate
    /// teardown (the re-identify dismissal) and for the import handle's
    /// cancellation of a decided candidate's identification, which ends its
    /// extraction beside its run; a removed or
    /// reshaped candidate cancels through the bus listener instead.
    pub fn cancel(&self, key: &str) {
        self.inner.cancellation.cancel(key);
    }
}

/// Drive extraction for one candidate. Builds the inputs for its source, then
/// streams `Signals` snapshots as the disc ID, barcodes, and text settle.
async fn run_extraction(
    inner: Arc<ExtractionServiceInner>,
    extraction: RunningExtraction,
    source: ExtractionSource,
    token: CancellationToken,
) {
    let _release = ExtractionRelease {
        inner: inner.clone(),
        key: extraction.key.clone(),
        generation: extraction.generation,
    };

    if token.is_cancelled() {
        return;
    }

    match source {
        // One scan derives every non-OCR signal in a single blocking hop, then
        // the artwork OCR streams.
        ExtractionSource::Candidate { candidate } => {
            let Some(fast) = run_fast_pass_blocking(&inner.runtime_handle, move || {
                gather_non_ocr_sources(&candidate.source_folders(), candidate.files())
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
                extraction,
                token,
                ExtractionInputs {
                    gathered: Gathered {
                        disc_id: fast.disc_id,
                        barcodes: fast.cue_barcodes,
                        pool,
                        durations: fast.durations,
                    },
                    artwork,
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
                extraction,
                token,
                ExtractionInputs {
                    gathered: Gathered {
                        disc_id,
                        barcodes: Vec::new(),
                        pool: Pool::default(),
                        // A library release has no candidate folder to walk, so
                        // nothing is probed on this path.
                        durations: crate::import::probe::SourceDurations::default(),
                    },
                    artwork,
                },
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

/// What the pass has gathered so far: the settled disc ID, every barcode found
/// (CUE first, then each image OCR adds to it), the text pool, and the folder's
/// track durations. Every snapshot the pass emits is built from this.
struct Gathered {
    disc_id: DiscIdSignal,
    barcodes: Vec<SourcedValue>,
    pool: Pool,
    durations: crate::import::probe::SourceDurations,
}

/// What the streaming pass consumes: what is already gathered, and the artwork
/// pass that adds to it. A folder scan and a release re-identify each build one,
/// differing only in which fields are populated.
struct ExtractionInputs {
    gathered: Gathered,
    artwork: Option<ArtworkPass>,
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
    extraction: RunningExtraction,
    token: CancellationToken,
    inputs: ExtractionInputs,
) {
    let ExtractionInputs {
        mut gathered,
        artwork,
    } = inputs;
    let total = artwork.as_ref().map_or(0, |pass| pass.images.len() as u32);
    let has_artwork = artwork.is_some();
    let position_of = |images: &[ArtworkImage], index: usize| ArtworkScan::Reading {
        current: images[index].file_id.clone(),
        position: index as u32 + 1,
        total,
    };

    if token.is_cancelled() {
        return;
    }

    // First snapshot, only when there is artwork to read: disc ID and CUE
    // barcodes are settled and the autocomplete pool is populated, while
    // barcode/text stay `Scanning` until the OCR pass has been over every
    // image. Without artwork nothing is scanned, so the settled snapshot below
    // is the first and only one: `Scanning` means artwork is being read.
    if let Some(pass) = &artwork {
        let classification = gathered.pool.classify();
        emit_signals(
            &inner,
            &extraction,
            scanning_signals(&gathered, classification.catalogs, classification.free_text),
            position_of(&pass.images, 0),
        );
    }

    // One OCR request at a time (Vision on the ANE is effectively serial).
    if let Some(ArtworkPass { images }) = artwork {
        for (index, ArtworkImage { path, file_id }) in images.iter().enumerate() {
            if token.is_cancelled() {
                return;
            }

            let analysis = match inner.analyze_artwork(path.clone()).await {
                Ok(analysis) => analysis,
                Err(failure) => {
                    emit_failed_ocr_signals(
                        &inner,
                        &extraction,
                        gathered,
                        ArtworkScan::Failed {
                            failure: failure.clone(),
                            read: index as u32,
                            total,
                        },
                        failure,
                    );
                    return;
                }
            };

            if token.is_cancelled() {
                return;
            }

            // Accumulate barcodes — one sighting per image a code was read
            // off, a code read twice off one image once — and text lines.
            for barcode in analysis.barcodes {
                // A run of one digit is printed on nothing; OCR reads them off
                // borders and shadows, and looking one up can only miss.
                if crate::signals::is_placeholder_code(&barcode.payload) {
                    continue;
                }
                let seen_here = gathered
                    .barcodes
                    .iter()
                    .any(|b| b.value == barcode.payload && &b.origin_path == file_id);
                if !seen_here {
                    // The image it was read off, so a surface can put the
                    // barcode on that image rather than beside the release.
                    let sighting = match file_id {
                        Some(file_id) => SourcedValue::in_file(
                            barcode.payload,
                            SignalOrigin::Artwork,
                            file_id.clone(),
                        ),
                        None => SourcedValue::new(barcode.payload, SignalOrigin::Artwork),
                    };
                    gathered.barcodes.push(sighting.at(barcode.region));
                }
            }
            for line in analysis.text_lines {
                gathered.pool.push(SourcedLine {
                    source: Source::Artwork {
                        path: path.clone(),
                        file_id: file_id.clone(),
                    },
                    text: line.text,
                    region: line.region,
                });
            }

            // The last image's snapshot is the settled one below: nothing is
            // being read any more, and saying so twice would be one snapshot
            // too many.
            if index + 1 == images.len() {
                break;
            }

            // Re-check cancellation before emitting; a successor's `start()` can
            // flip the token during the synchronous push/classify window.
            if token.is_cancelled() {
                return;
            }

            // Every image read is a snapshot, whether or not it added anything:
            // the pass has moved on to the next image, and that is what a
            // surface watching the run is shown.
            let classification = gathered.pool.classify();
            emit_signals(
                &inner,
                &extraction,
                scanning_signals(&gathered, classification.catalogs, classification.free_text),
                position_of(&images, index + 1),
            );
        }
    }

    if token.is_cancelled() {
        return;
    }

    let classification = gathered.pool.classify();
    let barcode = if has_artwork || !gathered.barcodes.is_empty() {
        BarcodeSignal::Settled {
            codes: gathered.barcodes,
        }
    } else {
        BarcodeSignal::Absent
    };
    emit_signals(
        &inner,
        &extraction,
        Signals {
            disc_id: gathered.disc_id,
            barcode,
            text: TextSignal::Settled {
                catalogs: classification.catalogs,
                free_text: classification.free_text,
            },
            text_pool: gathered.pool.text_lines(),
            durations: gathered.durations,
        },
        if has_artwork {
            ArtworkScan::Done { total }
        } else {
            ArtworkScan::Absent
        },
    );
}

fn emit_failed_ocr_signals(
    inner: &ExtractionServiceInner,
    extraction: &RunningExtraction,
    mut gathered: Gathered,
    artwork: ArtworkScan,
    failure: LookupFailure,
) {
    let classification = gathered.pool.classify();
    let barcode = BarcodeSignal::Failed {
        failure: failure.clone(),
        codes: gathered.barcodes,
    };
    emit_signals(
        inner,
        extraction,
        Signals {
            disc_id: gathered.disc_id,
            barcode,
            text: TextSignal::Failed {
                failure,
                catalogs: classification.catalogs,
                free_text: classification.free_text,
            },
            text_pool: gathered.pool.text_lines(),
            durations: gathered.durations,
        },
        artwork,
    );
}

/// Build a `Scanning`-phase `Signals` snapshot: what has been read so far
/// while the artwork pass is still going. Only an extraction with artwork
/// emits one; barcode and text both stay `Scanning` until the pass is over.
fn scanning_signals(
    gathered: &Gathered,
    catalogs: Vec<SourcedValue>,
    free_text: Vec<String>,
) -> Signals {
    let text_pool = gathered.pool.text_lines();
    Signals {
        disc_id: gathered.disc_id.clone(),
        barcode: BarcodeSignal::Scanning {
            codes: gathered.barcodes.clone(),
        },
        text: TextSignal::Scanning {
            catalogs,
            free_text,
        },
        text_pool,
        durations: gathered.durations.clone(),
    }
}

/// Put a `Signals` snapshot, with where the artwork pass has got to, on the
/// run's watch and on the import event bus — only while the extraction is
/// still its key's current one. The cancellation checks along the pass are
/// not enough on their own: a successor's `start` can land between a check
/// and the send, and a snapshot sent then would follow the successor's own on
/// the bus, naming a run that is over. The registry decides and sends under
/// one lock, so it cannot.
fn emit_signals(
    inner: &ExtractionServiceInner,
    extraction: &RunningExtraction,
    signals: Signals,
    artwork: ArtworkScan,
) {
    let key = &extraction.key;
    let sent = inner
        .cancellation
        .while_current(key, extraction.generation, || {
            extraction.snapshots.send_replace(Some(SignalsSnapshot {
                signals: signals.clone(),
                artwork: artwork.clone(),
            }));
            inner
                .event_tx
                .send(ImportEvent::SignalsUpdated {
                    candidate_key: key.clone(),
                    run: extraction.run,
                    signals,
                    artwork,
                    priority: extraction.priority,
                })
                .map_err(|err| err.to_string())
        });
    match sent {
        Some(Ok(_)) => {}
        Some(Err(err)) => {
            warn!("signals: SignalsUpdated broadcast had no subscribers for {key}: {err}");
        }
        None => debug!("signals: {key} extraction was replaced; its snapshot is not sent"),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
