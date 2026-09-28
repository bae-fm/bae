//! The signal-extraction service: one pass over a candidate's files that
//! streams whole [`Signals`] snapshots to the run it feeds (through an
//! [`ExtractionWatch`]) and to the import event bus.
//!
//! Everything that needs no OCR is read first, so the disc-ID lookup need not
//! wait on the artwork; then the images are read one at a time, a snapshot per
//! image, and the barcode and text signals settle at the end.
//!
//! An extraction that cannot gather its inputs sends one snapshot that fails
//! every signal, so its run settles as a failure instead of waiting. A
//! replaced extraction sends nothing more.

use super::analyzer::{ArtworkAnalysis, ArtworkAnalyzer};
use super::cancellation::CancellationRegistry;
use super::candidate_text::{Source, SourcedLine};
use super::fast_pass::{gather_non_ocr_sources, ArtworkImage, FastPass};
use super::pool::Pool;
use super::release::{resolve_release_artwork_paths, resolve_release_identity};
use crate::identify::IdentifyRunId;
use crate::import::{ImportEvent, ImportEventBus};
use crate::library::LibraryManager;
use crate::signals::{
    AudioFacts,
    ArtworkScan, AudioOrigin, BarcodeSignal, DiscIdSignal, LookupFailure, Signals, SourcedValue,
    TextSignal,
};
use crate::util::rate_limiter::CallPriority;
use crate::util::session_cache::SessionCache;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

/// Where a candidate's signals come from: a folder on disk, or an existing
/// library release being re-identified.
#[derive(Debug, Clone)]
pub enum ExtractionSource {
    Candidate {
        candidate: crate::import::folder_scanner::FolderCandidate,
    },
    Release {
        release_id: String,
    },
}

/// One snapshot of a candidate's signals, with the audio they were read
/// beside and where the artwork pass that produced them had got to.
#[derive(Debug, Clone)]
pub struct SignalsSnapshot {
    pub signals: Signals,
    pub audio: AudioFacts,
    pub artwork: ArtworkScan,
}

/// The latest snapshot of the extraction feeding a run, `None` until the
/// first; the sender dropping tells the run its extraction is over.
pub type ExtractionWatch = watch::Receiver<Option<SignalsSnapshot>>;

/// Thread-safe handle to the running signal-extraction service.
#[derive(Clone)]
pub struct ExtractionServiceHandle {
    inner: Arc<ExtractionServiceInner>,
}

struct ExtractionServiceInner {
    runtime_handle: tokio::runtime::Handle,
    event_tx: ImportEventBus,
    /// The platform's artwork analyzer; `None` where the platform has none, so
    /// artwork is no source at all.
    analyzer: Mutex<Option<Arc<dyn ArtworkAnalyzer>>>,
    /// Resolves a release's files for the `Release` re-identify path.
    library_manager: LibraryManager,
    /// Each folder's settled snapshot this session, so a later run over the
    /// same files does not read every image again.
    settled: SessionCache<SignalsSnapshot>,
    /// Per-candidate cancellation; the bus cancels a removed or rebound
    /// candidate's extraction in the send that says so.
    cancellation: CancellationRegistry,
}

/// One extraction in flight.
struct RunningExtraction {
    run: IdentifyRunId,
    key: String,
    /// The registry generation that says whether this is still the key's
    /// current extraction.
    generation: u64,
    priority: CallPriority,
    read_cover_art: bool,
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

/// How many folders' settled snapshots a session keeps.
const SETTLED_CAPACITY: usize = 1024;

pub struct ExtractionService;

impl ExtractionService {
    pub fn start(
        runtime_handle: tokio::runtime::Handle,
        event_tx: ImportEventBus,
        library_manager: LibraryManager,
    ) -> ExtractionServiceHandle {
        let inner = Arc::new(ExtractionServiceInner {
            runtime_handle,
            event_tx,
            analyzer: Mutex::new(None),
            library_manager,
            settled: SessionCache::new("Settled folder signals", SETTLED_CAPACITY),
            cancellation: CancellationRegistry::default(),
        });

        let ending = Arc::downgrade(&inner);
        inner.event_tx.on_candidate_ended(move |key| {
            if let Some(inner) = ending.upgrade() {
                inner.cancellation.cancel(key);
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
        .map_err(|_| LookupFailure::ArtworkAnalysis)
    }
}

impl ExtractionServiceHandle {
    /// Register the platform's artwork analyzer, once at boot.
    pub fn register_analyzer(&self, analyzer: Arc<dyn ArtworkAnalyzer>) {
        *self.inner.analyzer.lock().unwrap() = Some(analyzer);
    }

    /// Start extraction for candidate `key` from `source`, feeding `run`, and
    /// return the watch the run reads its snapshots off. Replaces any
    /// extraction in flight for the key. `priority` is the run's, carried on
    /// each snapshot; extraction itself calls no provider.
    pub fn start(
        &self,
        run: IdentifyRunId,
        key: String,
        source: ExtractionSource,
        priority: CallPriority,
        steps: crate::config::IdentificationSteps,
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
                    read_cover_art: steps.read_cover_art,
                    snapshots,
                };
                runtime_handle.spawn(async move {
                    run_extraction(inner, extraction, source, token).await;
                });
            });
        watch
    }

    /// Cancel a candidate's in-flight extraction.
    pub fn cancel(&self, key: &str) {
        self.inner.cancellation.cancel(key);
    }

    /// What ends once `key`'s extraction in flight is cancelled.
    #[cfg(test)]
    pub(crate) fn cancelled_for_test(
        &self,
        key: &str,
    ) -> Option<impl std::future::Future<Output = ()> + Send + 'static> {
        self.inner.cancellation.cancelled_for_test(key)
    }
}

/// Drive extraction for one candidate from its source.
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
        ExtractionSource::Candidate { candidate } => {
            let settled_key =
                settled_reading_key(&candidate.files.content_hash(), extraction.read_cover_art);
            if let Some(settled) = inner.settled.get_cloned(&settled_key) {
                debug!(
                    "signals: {} was read before with these files; reusing that reading",
                    extraction.key
                );
                emit_signals(&inner, &extraction, settled);
                return;
            }
            let fast = match run_fast_pass_blocking(&inner.runtime_handle, move || {
                gather_non_ocr_sources(&candidate.source_folders(), &candidate.files)
            })
            .await
            {
                Ok(fast) => fast,
                Err(detail) => {
                    let failure = LookupFailure::Diagnostic { detail };
                    emit_aborted_signals(
                        &inner,
                        &extraction,
                        DiscIdSignal::Failed {
                            failure: failure.clone(),
                        },
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
            let artwork = Artwork::unread(extraction.read_cover_art, inner.has_artwork_analyzer())
                .unwrap_or_else(|| Artwork::read(fast.artwork));
            let settled = stream_extraction(
                inner.clone(),
                extraction,
                token,
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
            if let Some(settled) = settled {
                inner.settled.put(settled_key, settled);
            }
        }

        // A library release has no folder text; its rip files and artwork come
        // from the library.
        ExtractionSource::Release { release_id } => {
            let (origin, audio, disc_id) =
                match resolve_release_identity(&inner.library_manager, &release_id).await {
                    Ok(identity) => (
                        identity.rip.origin,
                        identity.audio,
                        identity.rip.disc_id.into_signal(),
                    ),
                    Err(detail) => (
                        AudioOrigin::default(),
                        AudioFacts::default(),
                        DiscIdSignal::Failed {
                            failure: crate::signals::LookupFailure::Diagnostic { detail },
                        },
                    ),
                };
            if token.is_cancelled() {
                return;
            }
            // The artwork is resolved only when it will be read.
            // `_cover_staging` holds the staged cover's temp dir until
            // `stream_extraction` returns; a resolve error means the release's
            // files cannot be read at all, so the extraction aborts.
            let unread = Artwork::unread(extraction.read_cover_art, inner.has_artwork_analyzer());
            let (artwork, _cover_staging) = match unread {
                Some(unread) => (unread, None),
                None => {
                    match resolve_release_artwork_paths(&inner.library_manager, &release_id).await {
                        // A library release's images have no file id.
                        Ok((paths, staging)) => (
                            Artwork::read(
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
                            emit_aborted_signals(
                                &inner,
                                &extraction,
                                disc_id,
                                LookupFailure::Diagnostic { detail: e },
                            );
                            return;
                        }
                    }
                }
            };
            stream_extraction(
                inner,
                extraction,
                token,
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

/// The fast pass, or why the task died or the folder's timing does not read.
async fn run_fast_pass_blocking<F>(runtime_handle: &Handle, task: F) -> Result<FastPass, String>
where
    F: FnOnce() -> Result<FastPass, crate::import::ImportError> + Send + 'static,
{
    match run_blocking(runtime_handle, "fast-pass spawn_blocking failed", task).await? {
        Ok(pass) => Ok(pass),
        Err(error) => {
            error!("signals: folder timing is invalid: {error}; aborting extraction");
            Err(format!("folder timing is invalid: {error}"))
        }
    }
}

/// The blocking task's value, or why it panicked or was cancelled.
async fn run_blocking<T, F>(
    runtime_handle: &Handle,
    failure_context: &str,
    task: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    match runtime_handle.spawn_blocking(task).await {
        Ok(value) => Ok(value),
        Err(e) => {
            error!("signals: {failure_context}: {e}; aborting extraction");
            Err(format!("{failure_context}: {e}"))
        }
    }
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
    /// The run does not read cover art.
    Off,
    Read(ArtworkPass),
}

impl Artwork {
    /// What the pass does when it will read no image, decided before the
    /// images are looked for; `None` when they are to be read.
    fn unread(read_cover_art: bool, analyzer_available: bool) -> Option<Self> {
        if !analyzer_available {
            return Some(Artwork::Absent);
        }
        if !read_cover_art {
            return Some(Artwork::Off);
        }
        None
    }

    /// The pass over `images`, once [`Self::unread`] says they are to be read.
    fn read(images: Vec<ArtworkImage>) -> Self {
        if images.is_empty() {
            return Artwork::Absent;
        }
        Artwork::Read(ArtworkPass { images })
    }
}

/// The images to read, built only when there is an analyzer to read them.
struct ArtworkPass {
    /// Never empty.
    images: Vec<ArtworkImage>,
}

/// The session cache's key for a folder's settled reading: its files, and
/// whether the cover art was read.
fn settled_reading_key(content_hash: &str, read_cover_art: bool) -> String {
    if read_cover_art {
        content_hash.to_string()
    } else {
        format!("{content_hash} without cover art")
    }
}

/// Stream snapshots over the artwork pass — one before the first image and
/// one after each image but the last — then send and return the settled one.
/// `None` when the pass was cancelled or an image failed to read.
async fn stream_extraction(
    inner: Arc<ExtractionServiceInner>,
    extraction: RunningExtraction,
    token: CancellationToken,
    inputs: ExtractionInputs,
) -> Option<SignalsSnapshot> {
    let ExtractionInputs {
        mut gathered,
        artwork,
    } = inputs;
    let finished = match &artwork {
        Artwork::Absent => ArtworkScan::Absent,
        Artwork::Off => ArtworkScan::Off,
        Artwork::Read(pass) => ArtworkScan::Done {
            total: pass.images.len() as u32,
        },
    };
    let artwork = match artwork {
        Artwork::Read(pass) => Some(pass),
        Artwork::Absent | Artwork::Off => None,
    };
    let total = artwork.as_ref().map_or(0, |pass| pass.images.len() as u32);
    let has_artwork = artwork.is_some();
    let position_of = |images: &[ArtworkImage], index: usize| ArtworkScan::Reading {
        current: images[index].file_id.clone(),
        position: index as u32 + 1,
        total,
    };

    if token.is_cancelled() {
        return None;
    }

    // Without artwork the settled snapshot is the only one, so `Scanning`
    // always means images are being read.
    if let Some(pass) = &artwork {
        let classification = gathered.pool.classify();
        emit_signals(
            &inner,
            &extraction,
            SignalsSnapshot {
                signals: scanning_signals(
                    &gathered,
                    classification.catalogs,
                    classification.free_text,
                ),
                audio: gathered.audio.clone(),
                artwork: position_of(&pass.images, 0),
            },
        );
    }

    // One OCR request at a time (Vision on the ANE is effectively serial).
    if let Some(ArtworkPass { images }) = artwork {
        for (index, ArtworkImage { path, file_id }) in images.iter().enumerate() {
            if token.is_cancelled() {
                return None;
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
                    return None;
                }
            };

            if token.is_cancelled() {
                return None;
            }

            // One sighting per image a code was read off.
            for code in super::barcode::codes_in(&analysis) {
                let seen_here = gathered
                    .barcodes
                    .iter()
                    .any(|b| b.value == code.as_str() && &b.origin_path == file_id);
                if !seen_here {
                    gathered.barcodes.push(SourcedValue {
                        value: code.into_string(),
                        origin_path: file_id.clone(),
                    });
                }
            }
            for text in analysis.text_lines {
                gathered.pool.push(SourcedLine::new(
                    Source::Artwork { path: path.clone() },
                    text,
                ));
            }

            // The last image's snapshot is the settled one below.
            if index + 1 == images.len() {
                break;
            }

            if token.is_cancelled() {
                return None;
            }

            // Every image read is a snapshot, whether or not it added anything.
            let classification = gathered.pool.classify();
            emit_signals(
                &inner,
                &extraction,
                SignalsSnapshot {
                    signals: scanning_signals(
                        &gathered,
                        classification.catalogs,
                        classification.free_text,
                    ),
                    audio: gathered.audio.clone(),
                    artwork: position_of(&images, index + 1),
                },
            );
        }
    }

    if token.is_cancelled() {
        return None;
    }

    let classification = gathered.pool.classify();
    let barcode = if has_artwork || !gathered.barcodes.is_empty() {
        BarcodeSignal::Settled {
            codes: gathered.barcodes,
        }
    } else {
        BarcodeSignal::Absent
    };
    let settled = SignalsSnapshot {
        signals: Signals {
            origin: gathered.origin,
            disc_id: gathered.disc_id,
            barcode,
            text: TextSignal::Settled {
                catalogs: classification.catalogs,
                free_text: classification.free_text,
            },
            text_pool: gathered.pool.text_lines(),
            isrcs: gathered.isrcs,
            track_titles: gathered.track_titles,
        },
        audio: gathered.audio,
        artwork: finished,
    };
    emit_signals(&inner, &extraction, settled.clone());
    Some(settled)
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
        SignalsSnapshot {
            signals: Signals {
                origin: gathered.origin,
                disc_id: gathered.disc_id,
                barcode,
                text: TextSignal::Failed {
                    failure,
                    catalogs: classification.catalogs,
                    free_text: classification.free_text,
                },
                text_pool: gathered.pool.text_lines(),
                isrcs: gathered.isrcs,
                track_titles: gathered.track_titles,
            },
            audio: gathered.audio,
            artwork,
        },
    );
}

/// Send one snapshot with every signal failed, for an extraction that could
/// not gather its inputs, so its run settles as a failure. The audio could
/// not be read either, so it is none.
fn emit_aborted_signals(
    inner: &ExtractionServiceInner,
    extraction: &RunningExtraction,
    disc_id: DiscIdSignal,
    failure: LookupFailure,
) {
    emit_signals(
        inner,
        extraction,
        SignalsSnapshot {
            signals: Signals {
                origin: AudioOrigin::default(),
                disc_id,
                barcode: BarcodeSignal::Failed {
                    failure: failure.clone(),
                    codes: Vec::new(),
                },
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
            artwork: ArtworkScan::Failed {
                failure,
                read: 0,
                total: 0,
            },
        },
    );
}

/// What has been read so far while the artwork pass is still going.
fn scanning_signals(
    gathered: &Gathered,
    catalogs: Vec<String>,
    free_text: Vec<String>,
) -> Signals {
    let text_pool = gathered.pool.text_lines();
    Signals {
        origin: gathered.origin.clone(),
        disc_id: gathered.disc_id.clone(),
        barcode: BarcodeSignal::Scanning {
            codes: gathered.barcodes.clone(),
        },
        text: TextSignal::Scanning {
            catalogs,
            free_text,
        },
        text_pool,
        isrcs: gathered.isrcs.clone(),
        track_titles: gathered.track_titles.clone(),
    }
}

/// Send a snapshot to the run's watch and the bus while the extraction is
/// still its key's current one, checked under the registry's lock so a
/// successor starting mid-pass never sees a stale snapshot after its own.
fn emit_signals(
    inner: &ExtractionServiceInner,
    extraction: &RunningExtraction,
    snapshot: SignalsSnapshot,
) {
    let key = &extraction.key;
    let sent = inner
        .cancellation
        .while_current(key, extraction.generation, || {
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
        });
    if sent.is_none() {
        debug!("signals: {key} extraction was replaced; its snapshot is not sent");
    }
}

#[cfg(test)]
mod tests;
