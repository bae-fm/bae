//! The signal-extraction service: one pass over a candidate's files producing
//! a streamed [`Signals`] snapshot (disc ID, barcodes, classified text). Both
//! the identify pipeline (which looks the signals up and narrows matches) and
//! the search UI (which surfaces them) consume what this service emits.
//!
//! Emission is streamed so slow OCR doesn't gate the fast signals:
//!
//! 1. **Fast pass.** Everything that resolves without OCR — the disc ID
//!    (LOG/CUE), CUE `CATALOG` barcodes, and the non-OCR text sources
//!    (folder-name brackets, path components, filenames, CUE, NFO/TXT) — is
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

use crate::identify::analyzer::{ArtworkAnalyzer, NoopAnalyzer};
use crate::identify::candidate_text::{
    self, apply_free_text_cutoff, catalog_numbers_sourced, cluster_lines_incremental,
    extract_folder_brackets, parse_filename_stem, rank_clusters_in_place, strip_path_component,
    Cluster, Source, SourcedLine,
};
use crate::identify::discid::{resolve_release_artwork_paths, resolve_release_identity};
use crate::import::discid::compute_discid_from_categorized;
use crate::import::folder_scanner::{self, AudioContent, CategorizedFiles};
use crate::import::ImportEvent;
use crate::library::LibraryManager;
use crate::signals::{
    BarcodeSignal, DiscIdSignal, SignalOrigin, Signals, SourcedValue, TextSignal,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// Where a candidate's signals come from: a folder on disk, or an existing
/// library release being re-identified.
#[derive(Debug, Clone)]
pub enum ExtractionSource {
    Folder(PathBuf),
    Release { release_id: String },
}

/// Maximum size read from a single `.nfo` / `.txt` file. Caps pathological
/// inputs (e.g. a 10 MB booklet transcription) without blowing memory.
const MAX_TEXT_FILE_BYTES: u64 = 100 * 1024;

/// Thread-safe handle to the running signal-extraction service.
#[derive(Clone)]
pub struct ExtractionServiceHandle {
    inner: Arc<ExtractionServiceInner>,
}

struct ExtractionServiceInner {
    runtime_handle: tokio::runtime::Handle,
    event_tx: broadcast::Sender<ImportEvent>,
    clock: crate::clock::ClockRef,
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
        clock: crate::clock::ClockRef,
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
    /// CUE, NFO/TXT); a release re-identify resolves its files from the
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

/// Accumulating pool fed to the classifier pipeline. `lines` carries per-
/// source-tagged text for filter / cluster / rank. `bracket_catalogs` holds
/// folder-bracket extractions routed straight to the catalog pool (they
/// bypass the free-text catalog regex, which is too strict for real-world
/// formats like `Z1 12345`).
///
/// The pool keeps two caches alongside the lines:
///
/// * `seen` — dedup set for `push`, so inserts stay O(1) as the pool grows.
/// * `clusters` + `clustered_through` — persistent cluster state so
///   `classify` can do incremental work on newly-added lines instead of
///   re-clustering the entire pool each emission.
#[derive(Default)]
struct Pool {
    lines: Vec<SourcedLine>,
    bracket_catalogs: Vec<String>,
    seen: HashSet<(Source, String)>,
    clusters: Vec<Cluster>,
    clustered_through: usize,
}

impl Pool {
    fn push(&mut self, line: SourcedLine) {
        if line.text.is_empty() {
            return;
        }
        // Dedupe by (source, text) via the HashSet. Keeps the Vec ordered
        // by insertion so clustering walks in insertion order.
        let key = (line.source.clone(), line.text.clone());
        if !self.seen.insert(key) {
            return;
        }
        self.lines.push(line);
    }

    fn push_bracket(&mut self, s: String) {
        if !s.is_empty() && !self.bracket_catalogs.contains(&s) {
            self.bracket_catalogs.push(s);
        }
    }

    /// Classify the pool's current contents into `(catalogs, free_text)`.
    /// Catalog extraction is a whole-pool regex pass (cheap). Free-text
    /// clustering is incremental: only lines added since the last call are
    /// classified against the existing clusters.
    fn classify(&mut self) -> (Vec<SourcedValue>, Vec<String>) {
        // Catalogs: regex over the sourced line text (each survivor keeps its
        // line's origin), plus the bracket-routed extras from folder names.
        let mut catalogs = catalog_numbers_sourced(&self.lines);
        let mut seen_catalog: HashSet<String> = catalogs.iter().map(|c| c.value.clone()).collect();
        for extra in &self.bracket_catalogs {
            if seen_catalog.insert(extra.clone()) {
                catalogs.push(SourcedValue::new(extra.clone(), SignalOrigin::FolderName));
            }
        }

        // Free text: classify only the newly-added lines against existing
        // clusters. Lines rejected by `should_reject_line` don't count as
        // "classified" at all — they never feed a cluster.
        let new_slice = &self.lines[self.clustered_through..];
        let filtered: Vec<SourcedLine> = new_slice
            .iter()
            .filter(|l| !candidate_text::should_reject_line(&l.text))
            .cloned()
            .collect();
        cluster_lines_incremental(&mut self.clusters, &filtered);
        self.clustered_through = self.lines.len();

        // Rank + cutoff operate on a local copy so the pool's insertion-
        // order clusters aren't mutated for display.
        let mut ranked = self.clusters.clone();
        rank_clusters_in_place(&mut ranked);
        let free_text = apply_free_text_cutoff(&ranked);

        (catalogs, free_text)
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
                Err(message) => DiscIdSignal::Failed {
                    message,
                    track_count: 0,
                },
            };
            if token.is_cancelled() {
                remove_own_entry(&inner, &key, generation);
                return;
            }
            let artwork_paths = match resolve_release_artwork_paths(
                &inner.library_manager,
                &release_id,
            )
            .await
            {
                Ok(paths) => paths,
                Err(e) => {
                    warn!(
                            "signals: failed to resolve artwork for release {release_id}: {e}; extracting without cover art"
                        );
                    Vec::new()
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

/// Result of the fast-pass gather: every non-OCR signal a folder yields — the
/// disc ID, CUE-CATALOG barcodes, classified-text source lines + brackets —
/// plus the artwork paths for the OCR phase.
struct FastPass {
    lines: Vec<SourcedLine>,
    bracket_catalogs: Vec<String>,
    artwork_paths: Vec<PathBuf>,
    disc_id: DiscIdSignal,
    cue_barcodes: Vec<SourcedValue>,
}

impl FastPass {
    /// Empty pass — no disc ID, no sources. Used when the folder scan fails.
    fn empty() -> Self {
        Self {
            lines: Vec::new(),
            bracket_catalogs: Vec::new(),
            artwork_paths: Vec::new(),
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            cue_barcodes: Vec::new(),
        }
    }
}

/// CUE `CATALOG` payloads (the disc's UPC/EAN) from a folder's parsed sheets,
/// paired and unpaired, deduped — barcode-lookup inputs, not catalog-string
/// filter values. Each carries `SignalOrigin::CueSheet`.
fn cue_barcodes(categorized: &CategorizedFiles) -> Vec<SourcedValue> {
    let mut out: Vec<SourcedValue> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut consider = |catalog: &Option<String>| {
        if let Some(value) = catalog {
            let value = value.trim();
            if !value.is_empty() && seen.insert(value.to_string()) {
                out.push(SourcedValue::new(value.to_string(), SignalOrigin::CueSheet));
            }
        }
    };
    if let AudioContent::CueFlacPairs { pairs, .. } = &categorized.audio {
        for pair in pairs {
            if let Some(sheet) = &pair.cue_sheet {
                consider(&sheet.catalog);
            }
        }
    }
    for (_, sheet) in &categorized.unpaired_cue_sheets {
        consider(&sheet.catalog);
    }
    out
}

/// Synchronously enumerate and read all non-OCR sources. Runs on the
/// blocking pool via `spawn_blocking`. Any failure surfaces as missing data
/// for that source — it never aborts extraction.
fn gather_non_ocr_sources(folder: &Path) -> FastPass {
    let mut pass = FastPass::empty();

    // Folder path components: the candidate folder name + its immediate
    // parent (the plan's "last 2 path components" heuristic). Strip year /
    // track-number prefixes and trailing brackets.
    let folder_name = folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let parent_name = folder
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();

    for raw in [&parent_name, &folder_name] {
        if raw.is_empty() {
            continue;
        }
        if let Some(stripped) = strip_path_component(raw) {
            pass.lines.push(SourcedLine {
                source: Source::PathComponent,
                text: stripped,
            });
        }
        for bracket in extract_folder_brackets(raw) {
            pass.bracket_catalogs.push(bracket);
        }
    }

    // Enumerate files via the existing folder scanner. Failure here means
    // no audio was detected — we can still pull filenames / brackets from
    // the folder name, so we already captured that above.
    let categorized = match folder_scanner::collect_release_candidate_files(folder) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                "signals: folder scan failed for {:?}: {e}; using folder-name signals only",
                folder,
            );
            return pass;
        }
    };

    // Disc ID from LOG/CUE and CUE-CATALOG barcodes — derived from the same
    // parsed scan, no re-read.
    let track_count = categorized.audio.track_count().unwrap_or(0);
    pass.disc_id = match compute_discid_from_categorized(&categorized) {
        Some(disc_id) => DiscIdSignal::Computed {
            disc_id,
            track_count,
        },
        None => DiscIdSignal::Absent { track_count },
    };
    pass.cue_barcodes = cue_barcodes(&categorized);

    // Filenames — image + document only. Audio filenames are overwhelmingly
    // track titles, which are the wrong pool for Artist / Album autocomplete.
    // Generic names (`cover`, `booklet-01`, etc.) get rejected by
    // `parse_filename_stem`.
    for p in enumerate_filename_inputs(&categorized) {
        for part in parse_filename_stem(&p) {
            pass.lines.push(SourcedLine {
                source: Source::FilenameGeneric(p.clone()),
                text: part,
            });
        }
    }

    // CUE files — harvest PERFORMER / TITLE from the sheets the folder scan
    // already parsed: paired CUEs carry their sheet, multi-FILE and aggregate
    // CUEs land in `unpaired_cue_sheets`. No re-reading, no second parser.
    if let AudioContent::CueFlacPairs { pairs, .. } = &categorized.audio {
        for pair in pairs {
            if let Some(sheet) = &pair.cue_sheet {
                for name in cue_sheet_names(sheet) {
                    pass.lines.push(SourcedLine {
                        source: Source::CueField,
                        text: name,
                    });
                }
            }
        }
    }
    for (_, sheet) in &categorized.unpaired_cue_sheets {
        for name in cue_sheet_names(sheet) {
            pass.lines.push(SourcedLine {
                source: Source::CueField,
                text: name,
            });
        }
    }

    // NFO / TXT files — one line per `\n`, treated like OCR input.
    for doc in text_file_paths(&categorized) {
        if let Some(text) = read_capped_text(&doc) {
            for line in text.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    pass.lines.push(SourcedLine {
                        source: Source::TextFile(doc.clone()),
                        text: trimmed.to_string(),
                    });
                }
            }
        }
    }

    // Artwork paths for the OCR pass.
    pass.artwork_paths = categorized.artwork.into_iter().map(|f| f.path).collect();

    pass
}

/// Non-audio filename inputs the classifier should see.
///
/// Audio filenames are excluded: their stems are almost always track
/// titles, which belong in a track-title pool (not surfaced here), not the
/// Artist / Album autocomplete pool. CUE files are excluded for the same
/// reason — their `PERFORMER` / `TITLE` values are harvested as `CueField`
/// lines, while the filename stem (`Album.cue` → `Album`) would only
/// duplicate path-component signal at lower weight.
fn enumerate_filename_inputs(categorized: &CategorizedFiles) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for f in &categorized.artwork {
        out.push(f.path.clone());
    }
    for f in &categorized.documents {
        out.push(f.path.clone());
    }
    out
}

/// Album- and track-level PERFORMER / TITLE values from a parsed CUE sheet,
/// deduped — the name tokens for the Artist / Album autocomplete pool. The
/// folder scanner parses every CUE once; this reads that result rather than
/// re-scanning the file.
fn cue_sheet_names(sheet: &crate::cue_flac::CueSheet) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |value: &Option<String>| {
        if let Some(s) = value {
            let s = s.trim();
            if !s.is_empty() && seen.insert(s.to_string()) {
                out.push(s.to_string());
            }
        }
    };
    push(&sheet.performer);
    push(&sheet.title);
    for track in &sheet.tracks {
        push(&track.performer);
        push(&track.title);
    }
    out
}

/// Returns `.nfo` and `.txt` files from the documents list. Excludes `.log`
/// (rip-technical data with no artist/album content) and `.cue` (handled
/// separately).
fn text_file_paths(categorized: &CategorizedFiles) -> Vec<PathBuf> {
    categorized
        .documents
        .iter()
        .filter(|f| has_ext(&f.path, "nfo") || has_ext(&f.path, "txt"))
        .map(|f| f.path.clone())
        .collect()
}

fn has_ext(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

/// Read a text file for harvesting, capped at `MAX_TEXT_FILE_BYTES`, decoding
/// with the project's encoding detection (`text_encoding`: UTF-8, UTF-16 BOM,
/// legacy fallback via chardetng). Returns `None` only on an I/O error, which
/// is logged so silent skips show up in traces. A non-UTF-8 file is decoded,
/// not dropped.
fn read_capped_text(path: &Path) -> Option<String> {
    use std::io::{ErrorKind, Read};

    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            debug!(
                "candidate_text: file vanished after scan: {}",
                path.display()
            );
            return None;
        }
        Err(e) => {
            warn!("candidate_text: open failed for {}: {e}", path.display());
            return None;
        }
    };
    let mut buf = Vec::new();
    if let Err(e) = f.take(MAX_TEXT_FILE_BYTES).read_to_end(&mut buf) {
        warn!("candidate_text: read failed for {}: {e}", path.display());
        return None;
    }
    Some(crate::text_encoding::decode_text(&buf).text)
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
mod tests {
    use super::*;
    use crate::identify::analyzer::ArtworkAnalyzer;
    use crate::identify::ArtworkAnalysis;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;
    use tempfile::TempDir;

    /// Test analyzer: returns canned text lines keyed by filename (not full
    /// path, to stay portable across temp-dir paths). Optionally delays
    /// each call so cancellation can be exercised.
    struct StubAnalyzer {
        responses: StdMutex<HashMap<String, Vec<String>>>,
        delay: Option<Duration>,
    }

    impl StubAnalyzer {
        fn new() -> Self {
            Self {
                responses: StdMutex::new(HashMap::new()),
                delay: None,
            }
        }

        fn with(self, filename: &str, lines: Vec<String>) -> Self {
            self.responses
                .lock()
                .unwrap()
                .insert(filename.to_string(), lines);
            self
        }

        fn with_delay(mut self, delay: Duration) -> Self {
            self.delay = Some(delay);
            self
        }
    }

    impl ArtworkAnalyzer for StubAnalyzer {
        fn analyze(&self, path: &Path) -> ArtworkAnalysis {
            if let Some(delay) = self.delay {
                std::thread::sleep(delay);
            }
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let text_lines = self
                .responses
                .lock()
                .unwrap()
                .get(&filename)
                .cloned()
                .unwrap_or_default();
            ArtworkAnalysis {
                barcodes: Vec::new(),
                text_lines,
            }
        }
    }

    /// Drain events until we've seen the target count, or timeout. Filters
    /// to `SignalsUpdated` only, returning each snapshot's `Signals`.
    async fn collect_signals(
        rx: &mut broadcast::Receiver<ImportEvent>,
        expected: usize,
    ) -> Vec<Signals> {
        let mut out: Vec<Signals> = Vec::new();
        while out.len() < expected {
            let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("timed out collecting events")
                .expect("channel closed");
            if let ImportEvent::SignalsUpdated { signals, .. } = event {
                out.push(signals);
            }
        }
        out
    }

    /// Build a throwaway `LibraryManager` over a temp dir. The folder/CD
    /// extraction paths exercised here don't read from the library, but
    /// `ExtractionService::start` requires one. The returned `TempDir`
    /// must outlive the manager.
    async fn make_library_manager() -> (crate::library::LibraryManager, TempDir) {
        let tmp = TempDir::new().unwrap();
        let clock: crate::clock::ClockRef = Arc::new(crate::clock::SystemClock);
        let database = crate::db::Database::new_test(
            tmp.path().join("test.db").to_str().unwrap(),
            clock.clone(),
        )
        .await
        .unwrap();
        let library_dir = crate::library_dir::LibraryDir::new(tmp.path());
        // Unique id per test so keyring entries don't collide in the shared
        // process-global mock store (see `install_test_keyring`).
        let library_id = format!("test-{}", uuid::Uuid::new_v4());
        let config = crate::config::Config::with_defaults(
            library_id.clone(),
            "test-device".to_string(),
            library_dir.clone(),
            "Test Library".to_string(),
        );
        let config_handle = Arc::new(crate::config::ConfigHandle::new(config));
        crate::config::install_test_keyring();
        let key_service = crate::keys::KeyService::new(library_id);
        let manager = crate::library::LibraryManager::new(
            database,
            library_dir,
            config_handle,
            key_service,
            clock,
            Arc::new(crate::id_provider::UuidProvider),
            tokio::runtime::Handle::current(),
            None,
        );
        (manager, tmp)
    }

    /// Start a service and keep the library's temp dir alive for the test.
    async fn make_service() -> (
        ExtractionServiceHandle,
        broadcast::Receiver<ImportEvent>,
        TempDir,
    ) {
        let (tx, rx) = broadcast::channel(64);
        let (library_manager, lib_tmp) = make_library_manager().await;
        let handle = ExtractionService::start(
            tokio::runtime::Handle::current(),
            tx,
            Arc::new(crate::clock::SystemClock),
            library_manager,
        );
        (handle, rx, lib_tmp)
    }

    /// Minimal MP3 bytes — ID3v2 header. Enough for `is_valid_audio` to
    /// accept the file during folder categorization.
    fn minimal_mp3() -> Vec<u8> {
        // "ID3" magic + 7 bytes of padding is enough for the validator.
        let mut v = Vec::with_capacity(32);
        v.extend_from_slice(b"ID3");
        v.resize(32, 0);
        v
    }

    /// Minimal JPEG bytes — 0xFFD8FF magic + a byte. Enough for
    /// `is_valid_image` to accept it.
    fn minimal_jpeg() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00]
    }

    /// Build a release folder with one MP3 (to satisfy the audio gate),
    /// plus any images and documents the caller passes in. Returns the
    /// folder path under the temp dir.
    fn build_release(
        tmp: &TempDir,
        folder_name: &str,
        images: &[&str],
        documents: &[(&str, &str)],
    ) -> PathBuf {
        let folder = tmp.path().join(folder_name);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("01 - Track.mp3"), minimal_mp3()).unwrap();
        for img in images {
            fs::write(folder.join(img), minimal_jpeg()).unwrap();
        }
        for (name, content) in documents {
            fs::write(folder.join(name), content.as_bytes()).unwrap();
        }
        folder
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn emits_fast_pass_then_ocr_then_settled() {
        // Folder name carries a catalog-shaped bracket (XX34b), parent
        // carries an artist-shaped name. Images carry OCR lines.
        let tmp = TempDir::new().unwrap();
        let parent = tmp.path().join("Artist Name");
        fs::create_dir_all(&parent).unwrap();
        let folder = parent.join("1989 - Album Title [XX34b]");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("01 - Track.mp3"), minimal_mp3()).unwrap();
        fs::write(folder.join("Cover.jpg"), minimal_jpeg()).unwrap();
        fs::write(folder.join("Back.jpg"), minimal_jpeg()).unwrap();

        let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
            StubAnalyzer::new()
                .with("Cover.jpg", vec!["WPCR-80001".to_string()])
                .with("Back.jpg", vec!["Extra Line".to_string()]),
        );
        let (handle, mut rx, _lib_tmp) = make_service().await;
        handle.register_analyzer(analyzer);

        handle.start(
            "cand-1".to_string(),
            ExtractionSource::Folder(folder.clone()),
        );

        // Fast-pass snapshot + 2 OCR snapshots + final settled = 4 snapshots.
        let signals = collect_signals(&mut rx, 4).await;
        assert_eq!(signals.len(), 4);

        // Fast pass: folder-bracket `XX34b` lands in catalogs; path
        // components (score 3, above cutoff) land in free_text. Text is
        // still `Scanning` while artwork OCR is pending.
        assert!(
            matches!(signals[0].text, TextSignal::Scanning { .. }),
            "fast-pass text should be Scanning, got {:?}",
            signals[0].text,
        );
        assert!(
            signals[0]
                .text
                .catalogs()
                .iter()
                .any(|c| c.value == "XX34b"),
            "expected folder-bracket catalog in fast pass, got {:?}",
            signals[0].text.catalogs(),
        );
        assert!(
            signals[0]
                .text
                .free_text()
                .iter()
                .any(|s| s.contains("Artist Name") || s.contains("Album Title")),
            "expected folder/parent path components in fast pass, got {:?}",
            signals[0].text.free_text(),
        );

        // Artwork is processed in sorted order: Back.jpg, then Cover.jpg.
        // After Back.jpg: pool still dominated by path components; the
        // folder-bracket catalog is still present.
        assert!(signals[1]
            .text
            .catalogs()
            .iter()
            .any(|c| c.value == "XX34b"));

        // After Cover.jpg: WPCR-80001 joins catalogs.
        assert!(signals[2]
            .text
            .catalogs()
            .iter()
            .any(|c| c.value == "WPCR-80001"));

        // Final snapshot is settled with both catalogs.
        assert!(
            matches!(signals[3].text, TextSignal::Settled { .. }),
            "final text should be Settled, got {:?}",
            signals[3].text,
        );
        assert!(signals[3]
            .text
            .catalogs()
            .iter()
            .any(|c| c.value == "XX34b"));
        assert!(signals[3]
            .text
            .catalogs()
            .iter()
            .any(|c| c.value == "WPCR-80001"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn no_artwork_still_emits_fast_pass_and_settled() {
        let tmp = TempDir::new().unwrap();
        // No images, just a folder name with signals.
        let folder = build_release(&tmp, "Artist Name - Album Title", &[], &[]);

        let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
        let (handle, mut rx, _lib_tmp) = make_service().await;
        handle.register_analyzer(analyzer);

        handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

        // Fast pass + final settled.
        let signals = collect_signals(&mut rx, 2).await;
        assert_eq!(signals.len(), 2);
        assert!(matches!(signals[0].text, TextSignal::Scanning { .. }));
        assert!(matches!(signals[1].text, TextSignal::Settled { .. }));

        // With no artwork there's no barcode source — the signal is `Absent`
        // throughout, never `Scanning`.
        assert!(matches!(signals[0].barcode, BarcodeSignal::Absent));
        assert!(matches!(signals[1].barcode, BarcodeSignal::Absent));

        // Fast pass contains at least the folder-name path component.
        assert!(signals[0]
            .text
            .free_text()
            .iter()
            .any(|s| s.contains("Artist Name") || s.contains("Album Title")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn missing_folder_settles_gracefully() {
        // Service handles nonexistent folders — the scanner errors but the
        // service falls back to folder-name signals only. Here there's no
        // folder name to extract, so we still settle cleanly.
        let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
        let (handle, mut rx, _lib_tmp) = make_service().await;
        handle.register_analyzer(analyzer);

        handle.start(
            "cand-1".to_string(),
            ExtractionSource::Folder(PathBuf::from("/nonexistent-folder-path")),
        );

        // Fast pass (empty pool) + final settled.
        let signals = collect_signals(&mut rx, 2).await;
        assert!(matches!(signals[0].text, TextSignal::Scanning { .. }));
        assert!(matches!(
            signals[signals.len() - 1].text,
            TextSignal::Settled { .. }
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cue_fields_land_in_fast_pass() {
        let tmp = TempDir::new().unwrap();
        let folder = tmp.path().join("Some Folder");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("audio.mp3"), minimal_mp3()).unwrap();
        // CUE file with album-level + track-level values. The CUE's FILE
        // directive references a non-existent FLAC, which means this CUE
        // won't pair with the mp3 — it lands in documents and still gets
        // parsed by the fast pass.
        let cue = r#"PERFORMER "Artist Alpha"
TITLE "Album Title A"
FILE "audio.wav" WAVE
  TRACK 01 AUDIO
    PERFORMER "Artist Alpha"
    TITLE "Track One"
    INDEX 01 00:00:00
"#;
        fs::write(folder.join("Album.cue"), cue).unwrap();

        let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
        let (handle, mut rx, _lib_tmp) = make_service().await;
        handle.register_analyzer(analyzer);

        handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

        // Fast pass + final settled.
        let signals = collect_signals(&mut rx, 2).await;
        let fast = signals[0].text.free_text();
        assert!(
            fast.contains(&"Artist Alpha".to_string()),
            "fast pass missing CUE PERFORMER, got {fast:?}",
        );
        assert!(
            fast.contains(&"Album Title A".to_string()),
            "fast pass missing CUE TITLE, got {fast:?}",
        );
        assert!(
            fast.contains(&"Track One".to_string()),
            "fast pass missing CUE track TITLE, got {fast:?}",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cue_catalog_becomes_barcode() {
        // A CUE with a `CATALOG` field. The disc's UPC/EAN surfaces as a
        // barcode code, not as a catalog-number string. The CUE's FILE
        // directive references a non-existent WAV, so
        // it doesn't pair with the mp3 and lands in `unpaired_cue_sheets`;
        // its CATALOG is still harvested. Track count (1) matches the
        // on-disk audio (1 mp3), so the incomplete-rip guard doesn't fire.
        let tmp = TempDir::new().unwrap();
        let folder = tmp.path().join("Some Folder");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("audio.mp3"), minimal_mp3()).unwrap();
        let cue = "CATALOG 0075678164521\n\
PERFORMER \"Artist Alpha\"\n\
TITLE \"Album Title A\"\n\
FILE \"audio.wav\" WAVE\n  \
  TRACK 01 AUDIO\n    \
    TITLE \"Track One\"\n    \
    INDEX 01 00:00:00\n";
        fs::write(folder.join("Album.cue"), cue).unwrap();

        let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
        let (handle, mut rx, _lib_tmp) = make_service().await;
        handle.register_analyzer(analyzer);

        handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

        // Fast pass + final settled.
        let signals = collect_signals(&mut rx, 2).await;
        let final_signals = &signals[signals.len() - 1];
        assert!(
            final_signals
                .barcode
                .codes()
                .iter()
                .any(|c| c.value == "0075678164521"),
            "CUE CATALOG should surface as a barcode code, got {:?}",
            final_signals.barcode,
        );
    }

    #[test]
    fn non_utf8_cue_is_decoded_not_dropped() {
        // A Windows-1252 CUE: a curly apostrophe (byte 0x92) inside a track
        // title. The folder scanner parses CUEs through `text_encoding` (BOM /
        // UTF-8 / chardetng), so the non-UTF-8 byte is decoded rather than
        // dropping the whole sheet, and the names are harvested from the parsed
        // result.
        let tmp = TempDir::new().unwrap();
        let folder = tmp.path().join("Some Folder");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("audio.mp3"), minimal_mp3()).unwrap();

        let mut cue: Vec<u8> = Vec::new();
        cue.extend_from_slice(b"PERFORMER \"Artist Alpha\"\n");
        cue.extend_from_slice(b"TITLE \"Album Title A\"\n");
        cue.extend_from_slice(b"FILE \"audio.wav\" WAVE\n");
        cue.extend_from_slice(b"  TRACK 01 AUDIO\n");
        cue.extend_from_slice(b"    TITLE \"I Ain");
        cue.push(0x92); // Windows-1252 right single quotation mark
        cue.extend_from_slice(b"t Got No Heart\"\n");
        cue.extend_from_slice(b"    INDEX 01 00:00:00\n");
        fs::write(folder.join("Album.cue"), &cue).unwrap();

        let pass = gather_non_ocr_sources(&folder);
        let texts: Vec<&str> = pass.lines.iter().map(|l| l.text.as_str()).collect();

        assert!(
            texts.contains(&"Artist Alpha"),
            "non-UTF-8 CUE dropped entirely; got {texts:?}",
        );
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with("I Ain") && t.ends_with("t Got No Heart")),
            "non-ASCII track title not recovered; got {texts:?}",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn text_files_feed_free_text() {
        // Text-file content alone is score 1 per line, which won't clear
        // the primary cutoff when path components are present at score 3.
        // To verify the text-file content is flowing into the pipeline at
        // all, arrange for the text-file content to match a path component
        // — they'll cluster together and pick up the combined score.
        let tmp = TempDir::new().unwrap();
        let folder = build_release(
            &tmp,
            "Artist Alpha - Album Title B",
            &[],
            &[(
                "info.txt",
                "Artist Alpha - Album Title B\nSome Other Thing\n",
            )],
        );

        let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
        let (handle, mut rx, _lib_tmp) = make_service().await;
        handle.register_analyzer(analyzer);

        handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

        // Fast pass + final settled.
        let signals = collect_signals(&mut rx, 2).await;
        // The path component `Artist Alpha - Album Title B` clusters with
        // the identical line from info.txt. Cluster score: PathComponent(3)
        // + TextFile(1) = 4 → well above cutoff.
        let final_free_text = signals[signals.len() - 1].text.free_text();
        assert!(
            final_free_text
                .iter()
                .any(|s| s == "Artist Alpha - Album Title B"),
            "expected path/text-file cluster to survive, got {final_free_text:?}",
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cancelled_ocr_run_does_not_settle() {
        // A cancelled OCR run stops emitting and tears down — it never
        // reaches its final `Settled` snapshot. With three delayed images
        // and a cancel mid-OCR, the run must not produce a `TextSignal::
        // Settled` for the cancelled key.
        let tmp = TempDir::new().unwrap();
        let folder = build_release(&tmp, "Some Folder", &["p1.jpg", "p2.jpg", "p3.jpg"], &[]);

        let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
            StubAnalyzer::new()
                .with("p1.jpg", vec!["Artist A".to_string()])
                .with("p2.jpg", vec!["Artist B".to_string()])
                .with("p3.jpg", vec!["Artist C".to_string()])
                .with_delay(Duration::from_millis(100)),
        );
        let (handle, mut rx, _lib_tmp) = make_service().await;
        handle.register_analyzer(analyzer);

        handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

        tokio::time::sleep(Duration::from_millis(50)).await;
        handle.cancel("cand-1");

        // Drain for a short window after the cancel. The cancelled run does
        // not complete, so no `Settled` snapshot for this key ever arrives.
        loop {
            match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
                Ok(Ok(ImportEvent::SignalsUpdated { signals, .. })) => {
                    assert!(
                        !matches!(signals.text, TextSignal::Settled { .. }),
                        "cancelled OCR run must not emit a Settled snapshot, got {:?}",
                        signals.text,
                    );
                }
                Ok(Ok(_)) => continue,
                _ => break,
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn restart_for_same_key_cancels_prior_then_starts_fresh() {
        // Two `start` calls for the same key: the first is cancelled by the
        // second's insert. Cancelled runs don't settle; the completed run
        // does. We assert the last run reaches a `Settled` snapshot and the
        // generation-guarded teardown neither deadlocks nor panics.
        let tmp = TempDir::new().unwrap();
        let folder = build_release(&tmp, "Some Folder", &["p1.jpg"], &[]);

        let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
            StubAnalyzer::new()
                .with("p1.jpg", vec!["Artist A".to_string()])
                .with_delay(Duration::from_millis(100)),
        );
        let (handle, mut rx, _lib_tmp) = make_service().await;
        handle.register_analyzer(analyzer);

        handle.start(
            "cand-1".to_string(),
            ExtractionSource::Folder(folder.clone()),
        );
        handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

        // Drain the event window; at least one completed run must settle.
        let mut saw_settled = false;
        loop {
            match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
                Ok(Ok(ImportEvent::SignalsUpdated { signals, .. })) => {
                    if matches!(signals.text, TextSignal::Settled { .. }) {
                        saw_settled = true;
                    }
                }
                Ok(Ok(_)) => continue,
                _ => break,
            }
        }

        assert!(
            saw_settled,
            "expected a Settled snapshot from the completed run",
        );
    }

    /// The race we're guarding against: three consecutive `start` calls for
    /// the same key. Without per-task generations, the first task's
    /// cancellation-teardown could fire *after* the second `start` inserts
    /// its token and remove it, leaving the third `start` with nothing to
    /// cancel.
    ///
    /// To provoke the race we hold OCR long enough that the first task is
    /// still alive when we issue the third `start`. Cancelled runs don't
    /// emit a final snapshot, so we can't count them directly; instead we
    /// assert the completed run settles and the `(generation, token)`
    /// teardown guard neither deadlocks nor panics under the concurrent
    /// start/cancel/teardown interleaving.
    #[tokio::test(flavor = "multi_thread")]
    async fn three_starts_cancel_each_predecessor() {
        let tmp = TempDir::new().unwrap();
        let folder = build_release(&tmp, "Some Folder", &["p1.jpg"], &[]);

        let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
            StubAnalyzer::new()
                .with("p1.jpg", vec!["Artist A".to_string()])
                .with_delay(Duration::from_millis(200)),
        );
        let (handle, mut rx, _lib_tmp) = make_service().await;
        handle.register_analyzer(analyzer);

        handle.start(
            "cand-1".to_string(),
            ExtractionSource::Folder(folder.clone()),
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
        handle.start(
            "cand-1".to_string(),
            ExtractionSource::Folder(folder.clone()),
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
        handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

        // Drain until the channel goes quiet. A completed run produces a
        // `Settled` snapshot; cancelled runs tear down silently.
        let mut saw_settled = false;
        loop {
            match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
                Ok(Ok(ImportEvent::SignalsUpdated { signals, .. })) => {
                    if matches!(signals.text, TextSignal::Settled { .. }) {
                        saw_settled = true;
                    }
                }
                Ok(Ok(_)) => continue,
                _ => break,
            }
        }

        assert!(
            saw_settled,
            "expected a Settled snapshot from the completed run",
        );
    }

    #[test]
    fn enumerate_filename_inputs_skips_all_audio_and_cue() {
        use crate::import::folder_scanner::{AudioContent, ScannedCueFlacPair, ScannedFile};

        let cue = ScannedFile::new(
            PathBuf::from("/rel/Album.cue"),
            "Album.cue".to_string(),
            100,
        );
        let flac = ScannedFile::new(
            PathBuf::from("/rel/Album.flac"),
            "Album.flac".to_string(),
            5_000_000,
        );
        let cover = ScannedFile::new(
            PathBuf::from("/rel/Artist Name - Album.png"),
            "Artist Name - Album.png".to_string(),
            10_000,
        );
        let pair = ScannedCueFlacPair {
            cue_file: cue.clone(),
            audio_file: flac.clone(),
            cue_sheet: None,
            total_size: 5_000_100,
            total_size_label: "5 MB".to_string(),
        };
        let categorized = CategorizedFiles {
            audio: AudioContent::CueFlacPairs {
                pairs: vec![pair],
                format_label: "CUE+FLAC".to_string(),
            },
            artwork: vec![cover.clone()],
            documents: Vec::new(),
            unpaired_cue_sheets: Vec::new(),
        };

        let inputs = enumerate_filename_inputs(&categorized);
        assert!(
            !inputs.iter().any(|p| p == &cue.path),
            "CUE names come from the parsed sheet, not the filename pool; got {inputs:?}",
        );
        assert!(
            !inputs.iter().any(|p| p == &flac.path),
            "audio filename stems are almost always track titles — wrong pool \
             for Artist / Album autocomplete; got {inputs:?}",
        );
        assert!(
            inputs.iter().any(|p| p == &cover.path),
            "artwork filenames still contribute — `Artist Name - Album.png` \
             and similar carry real signal; got {inputs:?}",
        );
    }

    // MARK: - Pool::classify end-to-end (one-shot on a fresh pool)

    fn cue_line(text: &str) -> SourcedLine {
        SourcedLine {
            source: Source::CueField,
            text: text.to_string(),
        }
    }

    fn path_line(text: &str) -> SourcedLine {
        SourcedLine {
            source: Source::PathComponent,
            text: text.to_string(),
        }
    }

    fn artwork_line(path: &str, text: &str) -> SourcedLine {
        SourcedLine {
            source: Source::Artwork(PathBuf::from(path)),
            text: text.to_string(),
        }
    }

    /// Push lines + brackets into a fresh pool and classify once, projecting
    /// the catalog `SourcedValue`s back to their bare strings for assertions.
    fn classify_pool(lines: Vec<SourcedLine>, brackets: &[&str]) -> (Vec<String>, Vec<String>) {
        let mut pool = Pool::default();
        for line in lines {
            pool.push(line);
        }
        for b in brackets {
            pool.push_bracket((*b).to_string());
        }
        let (catalogs, free_text) = pool.classify();
        (catalogs.into_iter().map(|c| c.value).collect(), free_text)
    }

    #[test]
    fn classify_promotes_high_score_clusters() {
        // Three sources agree on "Artist Alpha" — the cluster outscores a
        // one-off OCR credit line, which the reject rules drop.
        let lines = vec![
            cue_line("Artist Alpha"),
            path_line("Artist Alpha"),
            artwork_line("/a.jpg", "Artist Alpha"),
            artwork_line("/b.jpg", "Engineered by Name One"),
        ];
        let (_catalogs, free_text) = classify_pool(lines, &[]);
        assert!(
            free_text.iter().any(|s| s == "Artist Alpha"),
            "expected Artist Alpha to dominate, got {free_text:?}",
        );
        assert!(
            !free_text.iter().any(|s| s.contains("Engineered by")),
            "credit line should have been filtered, got {free_text:?}",
        );
    }

    #[test]
    fn classify_includes_bracket_catalogs_in_catalog_pool() {
        let (catalogs, _) = classify_pool(vec![cue_line("Artist Alpha")], &["XX34b"]);
        assert_eq!(catalogs, vec!["XX34b".to_string()]);
    }

    #[test]
    fn classify_falls_back_when_no_cluster_clears_threshold() {
        // Single artwork image, all-singleton clusters. Should NOT return
        // empty — fall back to the top N by score.
        let lines = vec![
            artwork_line("/a.jpg", "Artist Alpha"),
            artwork_line("/a.jpg", "Album Title B"),
            artwork_line("/a.jpg", "Label Name"),
        ];
        let (_, free_text) = classify_pool(lines, &[]);
        assert!(!free_text.is_empty(), "expected fallback top-N pool");
    }

    #[test]
    fn sanitize_short_key_passes_through() {
        assert_eq!(
            sanitize_key_for_filename("abc_def-123"),
            "abc_def-123".to_string(),
        );
    }

    #[test]
    fn sanitize_replaces_unsafe_characters() {
        let out = sanitize_key_for_filename("/a/b.c d#e");
        assert!(out
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn sanitize_long_key_truncated_with_hash_suffix() {
        // Pathological: 2000-char input. Result must fit the cap and two
        // different long inputs must produce distinct filenames.
        let long_a: String = "a".repeat(2000);
        let mut long_b = String::from("b");
        long_b.push_str(&"a".repeat(1999));

        let out_a = sanitize_key_for_filename(&long_a);
        let out_b = sanitize_key_for_filename(&long_b);

        assert!(
            out_a.len() <= SANITIZED_FILENAME_CAP,
            "sanitized length {} exceeds cap {SANITIZED_FILENAME_CAP}",
            out_a.len(),
        );
        assert_ne!(
            out_a, out_b,
            "distinct long keys must map to distinct filenames"
        );
        assert!(out_a.ends_with(|c: char| c.is_ascii_hexdigit()));
    }
}
