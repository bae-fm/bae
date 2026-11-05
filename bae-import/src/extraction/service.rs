//! The signal-extraction service: one pass over a candidate's files that
//! streams whole [`Signals`] snapshots to the run it feeds (through an
//! [`ExtractionWatch`]) and to the import event bus.
//!
//! Everything that needs no OCR is read first, so the disc-ID lookup need not
//! wait on the artwork; then the images are read several at a time and taken
//! in their order, a snapshot per image, and the barcode and text signals
//! settle at the end.
//!
//! An extraction that cannot gather its inputs, or that panics, sends one
//! snapshot that fails every signal, so its run ends as bae's own error
//! instead of waiting. A replaced extraction sends nothing more.

use super::analyzer::ArtworkAnalyzer;
use super::fast_pass::{gather_non_ocr_sources, ArtworkImage, FastPass};
use super::pool::Pool;
use super::release::{resolve_release_artwork_paths, resolve_release_identity};
use crate::IdentifyRunId;
use crate::{CandidateRuntime, CandidateWork, ImportEvent, ImportEventBus};
use bae_catalogs::rate_limiter::CallPriority;
use bae_catalogs::{AudioOrigin, InternalFailure};
use bae_identify::signals::{
    ArtworkScan, AudioFacts, BarcodeSignal, DiscIdSignal, Signals, SourcedValue, TextSignal,
};
use bae_util::session_cache::SessionCache;
use std::sync::{Arc, Mutex};
use stream::stream_extraction;
use tokio::sync::{watch, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

mod stream;

/// Where a candidate's signals come from: a folder on disk, or an existing
/// library release being re-identified.
#[derive(Debug, Clone)]
pub enum ExtractionSource {
    Candidate { candidate: crate::folder_scanner::FolderCandidate },
    Release { release_id: String },
}

/// One snapshot of a candidate's signals, with the audio they were read
/// beside and where the artwork pass that produced them had got to.
#[derive(Debug, Clone)]
pub struct SignalsSnapshot {
    pub signals: Signals,
    pub audio: AudioFacts,
    pub artwork: ArtworkScan,
}

/// The latest snapshot of the extraction feeding a run, from the extraction
/// service that started it; closed once the extraction is over.
pub struct ExtractionWatch(watch::Receiver<Option<SignalsSnapshot>>);

impl ExtractionWatch {
    /// The latest snapshot, `None` before the first; the value seen from
    /// here on.
    pub fn snapshot(&mut self) -> Option<SignalsSnapshot> {
        self.0.borrow_and_update().clone()
    }

    /// Wait until a snapshot newer than the one last seen is sent; an error
    /// once the extraction is over.
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.0.changed().await
    }

    /// Wait until the latest snapshot satisfies `done`; an error once the
    /// extraction is over.
    #[cfg(test)]
    pub(crate) async fn wait_for(
        &mut self,
        done: impl FnMut(&Option<SignalsSnapshot>) -> bool,
    ) -> Result<(), watch::error::RecvError> {
        self.0.wait_for(done).await.map(drop)
    }

    /// A watch over `snapshots`, for a test that feeds a run its snapshots
    /// itself.
    #[cfg(test)]
    pub(crate) fn of_test_channel(snapshots: watch::Receiver<Option<SignalsSnapshot>>) -> Self {
        Self(snapshots)
    }
}

/// The signal-extraction service.
#[derive(Clone)]
pub struct ExtractionService {
    inner: Arc<ExtractionServiceInner>,
}

struct ExtractionServiceInner {
    runtime_handle: tokio::runtime::Handle,
    event_tx: ImportEventBus,
    /// The platform's artwork analyzer; `None` where the platform has none, so
    /// artwork is no source at all.
    analyzer: Mutex<Option<Arc<dyn ArtworkAnalyzer>>>,
    /// The library a `Release` re-identify reads the release's files from.
    store: bae_store::Store,
    /// The library a `Release` re-identify reads the release's rows from.
    records: bae_records::Records,
    /// Where a panic in a pass is reported as the bug it is.
    diagnostics: bae_diagnostics::Diagnostics,
    /// Each folder's settled snapshot this session, so a later run over the
    /// same files does not read every image again.
    settled: SessionCache<SignalsSnapshot>,
    /// The record the bus keeps each candidate in, which holds the
    /// candidate's extraction in flight.
    candidates: CandidateRuntime,
    /// `IMAGES_IN_FLIGHT` permits, one held by each image being read, so
    /// passes running at once share the limit.
    image_reads: Arc<Semaphore>,
}

/// One extraction in flight.
struct RunningExtraction {
    run: IdentifyRunId,
    key: crate::CandidateKey,
    /// Whether this is still the key's current extraction.
    generation: u64,
    priority: CallPriority,
    snapshots: watch::Sender<Option<SignalsSnapshot>>,
}

struct ExtractionRelease {
    inner: Arc<ExtractionServiceInner>,
    key: crate::CandidateKey,
    generation: u64,
}

impl Drop for ExtractionRelease {
    fn drop(&mut self) {
        // A drop may run during an unwind, where a second panic ends the
        // process: a poisoned lock is reported, not unwrapped.
        if let Err(error) = self.inner.candidates.release_work(
            CandidateWork::Extraction,
            &self.key,
            self.generation,
        ) {
            self.inner.diagnostics.report_bug("release a finished extraction", &error);
        }
    }
}

/// How many folders' settled snapshots a session keeps.
const SETTLED_CAPACITY: usize = 1024;

/// How many images the analyzer reads at once, across every pass. The
/// platform analyzer is not serial: on a Mac with the app's Vision settings,
/// 40 album images took 6.5 s read one at a time (0.16 s each) and 3.1 s read
/// four at a time. Only that was measured, and each read holds a decoded scan,
/// so candidates identified at once share the limit rather than each having
/// its own.
pub(crate) const IMAGES_IN_FLIGHT: usize = 4;

impl ExtractionService {
    /// `candidates` is the runtime `event_tx` records into.
    pub fn new(
        runtime_handle: tokio::runtime::Handle,
        event_tx: ImportEventBus,
        candidates: CandidateRuntime,
        store: bae_store::Store,
        records: bae_records::Records,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> ExtractionService {
        let inner = Arc::new(ExtractionServiceInner {
            runtime_handle,
            event_tx,
            analyzer: Mutex::new(None),
            store,
            records,
            diagnostics,
            settled: SessionCache::new("Settled folder signals", SETTLED_CAPACITY),
            candidates,
            image_reads: Arc::new(Semaphore::new(IMAGES_IN_FLIGHT)),
        });
        ExtractionService { inner }
    }
}

impl ExtractionService {
    /// Register the platform's artwork analyzer, once at boot.
    pub fn register_analyzer(&self, analyzer: Arc<dyn ArtworkAnalyzer>) {
        *self.inner.analyzer.lock().expect("analyzer lock poisoned") = Some(analyzer);
    }

    /// Start extraction for candidate `key` from `source`, feeding `run`, and
    /// return the watch the run reads its snapshots off. Replaces any
    /// extraction in flight for the key. `priority` is the run's, carried on
    /// each snapshot; extraction itself calls no provider.
    pub fn start(
        &self,
        run: IdentifyRunId,
        key: crate::CandidateKey,
        source: ExtractionSource,
        priority: CallPriority,
    ) -> ExtractionWatch {
        let inner = self.inner.clone();
        let runtime_handle = self.inner.runtime_handle.clone();
        let (snapshots, snapshots_rx) = watch::channel(None);
        self.inner.candidates.start_work(
            CandidateWork::Extraction,
            key.clone(),
            move |token, generation| {
                let extraction = RunningExtraction { run, key, generation, priority, snapshots };
                runtime_handle.spawn(async move {
                    run_extraction(inner, extraction, source, token).await;
                });
            },
        );
        ExtractionWatch(snapshots_rx)
    }

    /// Cancel a candidate's in-flight extraction.
    pub fn cancel(&self, key: &crate::CandidateKey) {
        self.inner.candidates.cancel_work(CandidateWork::Extraction, key);
    }

    /// What ends once `key`'s extraction in flight is cancelled.
    #[cfg(test)]
    pub(crate) fn cancelled_for_test(
        &self,
        key: &crate::CandidateKey,
    ) -> Option<impl std::future::Future<Output = ()> + Send + 'static> {
        self.inner.candidates.work_cancelled_for_test(CandidateWork::Extraction, key)
    }
}

/// Drive extraction for one candidate from its source. One that panics is
/// reported, and fails every signal, as one that cannot gather its inputs
/// does.
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
    let extracted = bae_util::task_failure::caught(
        "extract a candidate's signals",
        extract(&inner, &extraction, source, &token),
    )
    .await;
    if let Err(failed) = extracted {
        let failure = InternalFailure::from(inner.diagnostics.panicked(failed));
        emit_aborted_signals(
            &inner,
            &extraction,
            DiscIdSignal::Failed { failure: failure.clone() },
            failure,
        );
    }
}

/// [`run_extraction`]'s work.
async fn extract(
    inner: &Arc<ExtractionServiceInner>,
    extraction: &RunningExtraction,
    source: ExtractionSource,
    token: &CancellationToken,
) {
    if token.is_cancelled() {
        return;
    }

    match source {
        ExtractionSource::Candidate { candidate } => {
            let settled_key = candidate.files.content_hash();
            if let Some(settled) = inner.settled.get_cloned(&settled_key) {
                debug!(
                    "signals: {} was read before with these files; reusing that reading",
                    extraction.key
                );
                emit_signals(inner, extraction, settled);
                return;
            }
            let fast = match run_fast_pass_blocking(&inner.diagnostics, move || {
                gather_non_ocr_sources(&candidate.source_folders(), &candidate.files)
            })
            .await
            {
                Ok(fast) => fast,
                Err(detail) => {
                    let failure = InternalFailure { detail };
                    emit_aborted_signals(
                        inner,
                        extraction,
                        DiscIdSignal::Failed { failure: failure.clone() },
                        failure,
                    );
                    return;
                }
            };
            let mut pool = Pool::default();
            for line in fast.lines {
                pool.push(line);
            }
            for catalog in fast.bracket_catalogs {
                pool.push_bracket(catalog);
            }
            let analyzer = inner.analyzer.lock().expect("analyzer lock poisoned").clone();
            let artwork = match analyzer {
                Some(analyzer) => Artwork::read(analyzer, fast.artwork),
                None => Artwork::Absent,
            };
            stream_extraction(
                inner,
                extraction,
                token,
                Some(settled_key),
                ExtractionInputs {
                    gathered: Gathered {
                        origin: fast.origin,
                        disc_id: fast.disc_id,
                        barcodes: fast.cue_barcodes,
                        pool,
                        audio: fast.audio,
                        isrcs: fast.isrcs,
                        track_titles: fast.track_titles,
                    },
                    artwork,
                },
            )
            .await;
        }

        // A library release has no folder text; its rip files and artwork come
        // from the library.
        ExtractionSource::Release { release_id } => {
            let (origin, audio, disc_id) =
                match resolve_release_identity(&inner.records, &inner.diagnostics, &release_id)
                    .await
                {
                    Ok(identity) => {
                        (identity.rip.origin, identity.audio, identity.rip.disc_id.into_signal())
                    }
                    Err(error) => (
                        AudioOrigin::default(),
                        AudioFacts::default(),
                        DiscIdSignal::Failed {
                            failure: InternalFailure::logged(
                                &format!("resolving release {release_id}"),
                                error,
                            ),
                        },
                    ),
                };
            if token.is_cancelled() {
                return;
            }
            // The artwork is resolved only when there is an analyzer to read
            // it. `_cover_staging` holds the staged cover's temp dir until
            // `stream_extraction` returns; a resolve error means the release's
            // files cannot be read at all, so the extraction aborts.
            let analyzer = inner.analyzer.lock().expect("analyzer lock poisoned").clone();
            let (artwork, _cover_staging) = if let Some(analyzer) = analyzer {
                match resolve_release_artwork_paths(
                    &inner.store,
                    &inner.records,
                    &inner.diagnostics,
                    &release_id,
                )
                .await
                {
                    // A library release's images have no file id.
                    Ok((paths, staging)) => (
                        Artwork::read(
                            analyzer,
                            paths
                                .into_iter()
                                .map(|path| ArtworkImage { path, file_id: None })
                                .collect(),
                        ),
                        staging,
                    ),
                    Err(e) => {
                        error!("signals: cannot read release {release_id} for artwork: {e}; aborting extraction");
                        emit_aborted_signals(
                            inner,
                            extraction,
                            disc_id,
                            InternalFailure { detail: e.to_string() },
                        );
                        return;
                    }
                }
            } else {
                (Artwork::Absent, None)
            };
            stream_extraction(
                inner,
                extraction,
                token,
                None,
                ExtractionInputs {
                    gathered: Gathered {
                        origin,
                        disc_id,
                        barcodes: Vec::new(),
                        pool: Pool::default(),
                        audio,
                        isrcs: Vec::new(),
                        track_titles: Vec::new(),
                    },
                    artwork,
                },
            )
            .await;
        }
    }
}

/// The fast pass, or why it panicked or the folder's timing does not read. A
/// panic is reported to `diagnostics`.
async fn run_fast_pass_blocking<F>(
    diagnostics: &bae_diagnostics::Diagnostics,
    task: F,
) -> Result<FastPass, String>
where
    F: FnOnce() -> Result<FastPass, crate::ImportError> + Send + 'static,
{
    let pass = bae_util::task_failure::blocking("read a folder's signals", task)
        .await
        .map_err(|failed| diagnostics.bug(failed).to_string())?;
    pass.map_err(|error| {
        error!("signals: folder timing is invalid: {error}; aborting extraction");
        format!("folder timing is invalid: {error}")
    })
}

/// What the pass has gathered so far, which every snapshot is built from.
/// Barcodes are the CUE's first, then each image's.
struct Gathered {
    origin: AudioOrigin,
    disc_id: DiscIdSignal,
    barcodes: Vec<SourcedValue>,
    pool: Pool,
    audio: AudioFacts,
    isrcs: Vec<String>,
    track_titles: Vec<String>,
}

/// What the streaming pass consumes: what is already gathered, and the
/// artwork that adds to it.
struct ExtractionInputs {
    gathered: Gathered,
    artwork: Artwork,
}

/// What the pass does with the candidate's artwork.
enum Artwork {
    /// No images, or no analyzer to read them with.
    Absent,
    Read(ArtworkPass),
}

impl Artwork {
    /// The pass over `images` with `analyzer`.
    fn read(analyzer: Arc<dyn ArtworkAnalyzer>, images: Vec<ArtworkImage>) -> Self {
        if images.is_empty() {
            return Artwork::Absent;
        }
        Artwork::Read(ArtworkPass { analyzer, images })
    }
}

/// The images to read and the analyzer that reads them.
struct ArtworkPass {
    analyzer: Arc<dyn ArtworkAnalyzer>,
    /// Never empty.
    images: Vec<ArtworkImage>,
}

/// Send one snapshot with every signal failed, for an extraction that could
/// not gather its inputs, so its run ends as an error. The audio could
/// not be read either, so it is none.
fn emit_aborted_signals(
    inner: &ExtractionServiceInner,
    extraction: &RunningExtraction,
    disc_id: DiscIdSignal,
    failure: InternalFailure,
) {
    emit_signals(
        inner,
        extraction,
        SignalsSnapshot {
            signals: Signals {
                origin: AudioOrigin::default(),
                disc_id,
                barcode: BarcodeSignal::Failed { failure: failure.clone(), codes: Vec::new() },
                text: TextSignal::Failed {
                    failure: failure.clone(),
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            audio: AudioFacts::default(),
            artwork: ArtworkScan::Failed { failure, read: 0, total: 0 },
        },
    );
}

/// Send a snapshot to the run's watch and the bus while the extraction is
/// still its key's current one, so a successor starting mid-pass never sees a
/// stale snapshot after its own.
fn emit_signals(
    inner: &ExtractionServiceInner,
    extraction: &RunningExtraction,
    snapshot: SignalsSnapshot,
) {
    let key = &extraction.key;
    let sent = inner.candidates.while_work_current(
        CandidateWork::Extraction,
        key,
        extraction.generation,
        || {
            let signals = snapshot.signals.clone();
            let artwork = snapshot.artwork.clone();
            extraction.snapshots.send_replace(Some(snapshot));
            inner.event_tx.send(ImportEvent::SignalsUpdated {
                candidate_key: key.clone(),
                run: extraction.run,
                signals,
                artwork,
                priority: extraction.priority,
            });
        },
    );
    if sent.is_none() {
        debug!("signals: {key} extraction was replaced; its snapshot is not sent");
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
