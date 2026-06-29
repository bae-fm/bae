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

use super::fast_pass::{gather_non_ocr_sources, FastPass};
use super::pool::Pool;
use crate::identify::analyzer::{ArtworkAnalyzer, NoopAnalyzer};
use crate::identify::candidate_text::{self, Source, SourcedLine};
use crate::identify::discid::{resolve_release_artwork_paths, resolve_release_identity};
use crate::import::ImportEvent;
use crate::library::LibraryManager;
use crate::signals::{
    BarcodeSignal, DiscIdSignal, SignalOrigin, Signals, SourcedValue, TextSignal,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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
    /// Map keyed by candidate key. Each entry carries the task's generation
    /// plus its cancellation token. Tasks only remove their entry if the
    /// stored generation still matches — avoids a race where an already-
    /// cancelled task races its successor on the way out and clears the
    /// wrong entry.
    cancel_tokens: Mutex<HashMap<String, (u64, CancellationToken)>>,
    /// Monotonic counter for generating task generations. Atomic so `start`
    /// can allocate a fresh id without holding the tokens lock.
    next_generation: AtomicU64,
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
                cancel_tokens: Mutex::new(HashMap::new()),
                next_generation: AtomicU64::new(0),
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
        let token = CancellationToken::new();
        let generation = self.inner.next_generation.fetch_add(1, Ordering::Relaxed);

        // Swap in the new entry atomically, cancelling any prior one — this
        // keeps start() consistent with cancel() even when a previous task
        // is mid-teardown.
        let prior = self
            .inner
            .cancel_tokens
            .lock()
            .unwrap()
            .insert(key.clone(), (generation, token.clone()));
        if let Some((_, prior_token)) = prior {
            prior_token.cancel();
        }

        let inner = self.inner.clone();
        self.inner.runtime_handle.spawn(async move {
            run_extraction(inner, key, source, token, generation).await;
        });
    }

    pub fn cancel(&self, key: &str) {
        let entry = self.inner.cancel_tokens.lock().unwrap().remove(key);
        if let Some((_, token)) = entry {
            token.cancel();
        }
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
    if token.is_cancelled() {
        remove_own_entry(&inner, &key, generation);
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
                generation,
                fast.disc_id,
                fast.cue_barcodes,
                pool,
                fast.artwork_paths,
                Some(folder),
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
                remove_own_entry(&inner, &key, generation);
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
                    remove_own_entry(&inner, &key, generation);
                    return;
                }
            };
            stream_extraction(
                inner,
                key,
                token,
                generation,
                disc_id,
                Vec::new(),
                Pool::default(),
                artwork_paths,
                None,
            )
            .await;
        }
    }
}

/// Stream `Signals` over the artwork OCR pass: emit the fast-pass snapshot,
/// then one cumulative snapshot per image that adds a barcode or text line,
/// then a final settled snapshot. `dump_folder` is `Some` for folder sources —
/// the diagnostic corpus dump runs after the final emit.
#[allow(clippy::too_many_arguments)]
async fn stream_extraction(
    inner: Arc<ExtractionServiceInner>,
    key: String,
    token: CancellationToken,
    generation: u64,
    disc_id: DiscIdSignal,
    mut barcodes: Vec<SourcedValue>,
    mut pool: Pool,
    artwork_paths: Vec<PathBuf>,
    dump_folder: Option<PathBuf>,
) {
    let has_artwork = !artwork_paths.is_empty();

    if token.is_cancelled() {
        remove_own_entry(&inner, &key, generation);
        return;
    }

    // First snapshot: disc ID and CUE barcodes are already settled and the
    // autocomplete pool is populated. Barcode/text stay `Scanning` while
    // artwork OCR is pending.
    let (catalogs, free_text) = pool.classify();
    emit_signals(
        &inner,
        &key,
        scanning_signals(disc_id.clone(), &barcodes, has_artwork, catalogs, free_text),
    );

    // One OCR request at a time (Vision on the ANE is effectively serial).
    for path in artwork_paths {
        if token.is_cancelled() {
            remove_own_entry(&inner, &key, generation);
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
            remove_own_entry(&inner, &key, generation);
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
            remove_own_entry(&inner, &key, generation);
            return;
        }

        let (catalogs, free_text) = pool.classify();
        emit_signals(
            &inner,
            &key,
            scanning_signals(disc_id.clone(), &barcodes, has_artwork, catalogs, free_text),
        );
    }

    if token.is_cancelled() {
        remove_own_entry(&inner, &key, generation);
        return;
    }

    // Final settled snapshot.
    let (catalogs, free_text) = pool.classify();
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

    remove_own_entry(&inner, &key, generation);

    // Diagnostic dump runs after the final emit — the UI already has its
    // final state, so a slow filesystem write doesn't delay completion.
    if let Some(folder) = dump_folder {
        let dump_now = inner.clock.now();
        let dump_key = key;
        let dump_pool = pool;
        tokio::task::spawn_blocking(move || {
            if let Err(e) = dump_scan(
                &dump_key, &folder, &dump_pool, &catalogs, &free_text, dump_now,
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

/// Remove the token entry for `key` only if it still refers to this task's
/// `generation`. Prevents a teardown from an older task erasing the entry
/// for a newer task that already overwrote it.
fn remove_own_entry(inner: &ExtractionServiceInner, key: &str, generation: u64) {
    let mut tokens = inner.cancel_tokens.lock().unwrap();
    if let Some((current_generation, _)) = tokens.get(key) {
        if *current_generation == generation {
            tokens.remove(key);
        }
    }
}

// ── Dump ────────────────────────────────────────────────────────────────────

/// Dump the completed scan to `~/.bae/candidate-text-scans/<key>.json` for
/// debugging and regression-corpus building. Records the raw sources, the
/// cluster pipeline intermediate, and the final classified output.
///
/// Returns `Err` if the dump directory can't be created or the file can't
/// be written. Callers log-and-swallow; dumps must never block emission.
fn dump_scan(
    key: &str,
    folder: &Path,
    pool: &Pool,
    catalogs: &[SourcedValue],
    free_text: &[String],
    now: chrono::DateTime<chrono::Utc>,
) -> std::io::Result<()> {
    let Some(home) = dirs::home_dir() else {
        warn!("candidate_text: no home dir, skipping dump");
        return Ok(());
    };
    let dir = home.join(".bae").join("candidate-text-scans");
    std::fs::create_dir_all(&dir)?;

    let filename = sanitize_key_for_filename(key);
    let path = dir.join(format!("{filename}.json"));

    let clusters = candidate_text::rank_clusters(candidate_text::cluster_lines(
        pool.lines
            .iter()
            .filter(|l| !candidate_text::should_reject_line(&l.text))
            .cloned()
            .collect(),
    ));
    let rejected: Vec<_> = pool
        .lines
        .iter()
        .filter(|l| candidate_text::should_reject_line(&l.text))
        .collect();

    let json = build_dump_json(
        key, folder, pool, &clusters, &rejected, catalogs, free_text, now,
    );
    std::fs::write(path, serde_json::to_vec_pretty(&json)?)?;
    Ok(())
}

/// Cap on the sanitized filename length. Keeps the dump path well under
/// most filesystems' 255-byte limit even after the fixed prefix/suffix.
const SANITIZED_FILENAME_CAP: usize = 200;

/// Replace characters unsafe for filenames with underscores. The candidate
/// key is typically a folder path.
///
/// When sanitization leaves a name longer than `SANITIZED_FILENAME_CAP`,
/// truncate and append an 8-hex-char hash of the original key so distinct
/// long keys still produce distinct filenames.
fn sanitize_key_for_filename(key: &str) -> String {
    let sanitized: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.len() <= SANITIZED_FILENAME_CAP {
        return sanitized;
    }

    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(key.as_bytes());
    let suffix: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    // Leave room for the `__` separator + 8 hex chars.
    let head_len = SANITIZED_FILENAME_CAP.saturating_sub(2 + suffix.len());
    let mut head: String = sanitized.chars().take(head_len).collect();
    head.push_str("__");
    head.push_str(&suffix);
    head
}

fn build_dump_json(
    key: &str,
    folder: &Path,
    pool: &Pool,
    clusters: &[candidate_text::Cluster],
    rejected: &[&SourcedLine],
    catalogs: &[SourcedValue],
    free_text: &[String],
    now: chrono::DateTime<chrono::Utc>,
) -> serde_json::Value {
    use serde_json::json;

    let catalogs_json: Vec<serde_json::Value> = catalogs
        .iter()
        .map(|c| {
            json!({
                "value": c.value,
                "origin": format!("{:?}", c.origin),
            })
        })
        .collect();

    let mut sources_artwork: HashMap<PathBuf, Vec<String>> = HashMap::new();
    let mut path_components: Vec<String> = Vec::new();
    let mut filenames: Vec<serde_json::Value> = Vec::new();
    let mut cue_fields: Vec<String> = Vec::new();
    let mut text_files: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for line in &pool.lines {
        match &line.source {
            Source::Artwork(p) => {
                sources_artwork
                    .entry(p.clone())
                    .or_default()
                    .push(line.text.clone());
            }
            Source::PathComponent => path_components.push(line.text.clone()),
            Source::FilenameGeneric(p) => {
                filenames.push(json!({
                    "path": p.to_string_lossy(),
                    "stem": line.text,
                }));
            }
            Source::CueField => cue_fields.push(line.text.clone()),
            Source::TextFile(p) => {
                text_files
                    .entry(p.clone())
                    .or_default()
                    .push(line.text.clone());
            }
        }
    }

    let artwork_json: Vec<serde_json::Value> = sources_artwork
        .into_iter()
        .map(|(path, lines)| {
            json!({
                "path": path.to_string_lossy(),
                "lines": lines,
            })
        })
        .collect();
    let text_files_json: Vec<serde_json::Value> = text_files
        .into_iter()
        .map(|(path, lines)| {
            json!({
                "path": path.to_string_lossy(),
                "lines": lines,
            })
        })
        .collect();

    let clusters_json: Vec<serde_json::Value> = clusters
        .iter()
        .map(|c| {
            json!({
                "representative": c.pick_representative(),
                "score": c.score(),
                "members": c.members.iter().map(source_line_to_json).collect::<Vec<_>>(),
            })
        })
        .collect();

    let rejected_json: Vec<serde_json::Value> = rejected
        .iter()
        .map(|l| {
            json!({
                "source": source_to_json(&l.source),
                "text": l.text,
            })
        })
        .collect();

    json!({
        "candidate": key,
        "folder": folder.to_string_lossy(),
        "finished_at": now.to_rfc3339(),
        "sources": {
            "artwork": artwork_json,
            "path_components": path_components,
            "folder_brackets": pool.bracket_catalogs,
            "filenames": filenames,
            "cue": cue_fields,
            "text_files": text_files_json,
        },
        "pipeline": {
            "clusters": clusters_json,
            "rejected": rejected_json,
        },
        "classified": {
            "catalogs": catalogs_json,
            "free_text": free_text,
        },
    })
}

fn source_to_json(source: &Source) -> serde_json::Value {
    use serde_json::json;
    match source {
        Source::Artwork(p) => json!({ "artwork": p.to_string_lossy() }),
        Source::PathComponent => json!("path_component"),
        Source::FilenameGeneric(p) => json!({ "filename_generic": p.to_string_lossy() }),
        Source::CueField => json!("cue_field"),
        Source::TextFile(p) => json!({ "text_file": p.to_string_lossy() }),
    }
}

fn source_line_to_json(line: &SourcedLine) -> serde_json::Value {
    use serde_json::json;
    json!({
        "source": source_to_json(&line.source),
        "text": line.text,
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
