//! Classification of candidate text into per-field suggestion pools.
//!
//! Input is a set of text lines harvested from one candidate's surfaces —
//! artwork OCR is the most noisy source but path components, folder-name
//! brackets, filenames, CUE sheets, and `.txt` content all feed in.
//! This module is intentionally pure: no Vision, no I/O, just string
//! transforms.
//!
//! Two pools feed the import search UI:
//!
//! * catalog numbers — likely catalog numbers (regex-extracted substrings),
//!   via `catalog_numbers_sourced`.
//! * free text — lines suitable for Artist / Album autocomplete, with
//!   whole-line barcodes and catalog numbers dropped by `should_reject_line`
//!   before clustering.
//!
//! Extractors for non-OCR sources:
//!
//! * `extract_folder_brackets` — bracketed substrings from folder names,
//!   filtered to the shape of real catalog numbers. Routes directly to the
//!   catalog pool (bypasses the free-text catalog regex, which is too strict
//!   for real-world formats like `Z1 12345` or `XYZ CD6`).
//! * `strip_path_component` — strips year prefixes, track-number prefixes,
//!   and trailing bracketed tails from a path segment.
//! * `parse_filename_stem` — file stem (no extension, no leading track
//!   number), with audio filenames additionally split on ` - `.

use crate::signals::{SignalOrigin, SourcedValue};
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use strsim::jaro_winkler;
use unicode_normalization::UnicodeNormalization;

/// Extract catalog-number-like substrings with provenance. Two regexes cover
/// the majority of real-world formats:
///
/// * Letter-prefix catalogs (`\b[A-Z]{2,6}[- ]?\d{3,7}\b`): `WPCR-80001`,
///   `COCQ 84487`, `TOCP12345`, `RR-500`.
/// * Single-letter-digit prefix (`\b[A-Z]\d[- ]?\d{4,7}\b`): `Z1 12345`,
///   `T5 67890`. Extra-constrained suffix length (≥4 digits) so we don't
///   pick up short incidentals.
///
/// Inner separators are preserved verbatim; MusicBrainz indexes `WPCR-80001`
/// and `WPCR 80001` as distinct tokens, so we keep both forms when both
/// appear. ZIP-code false positives (state abbrev + 5-digit code at a US
/// mailing-address tail) are rejected. Dedupes by first-seen order.
///
/// Each survivor is attributed to its line's
/// [`SignalOrigin::from_text_source`]. The catalog pool feeds the Refine
/// badges, which need to show where each candidate came from.
pub fn catalog_numbers_sourced(lines: &[SourcedLine]) -> Vec<SourcedValue> {
    let mut out: Vec<SourcedValue> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for line in lines {
        let origin = SignalOrigin::from_text_source(&line.source);
        for s in find_catalogs_in_line(&line.text) {
            if seen.insert(s.clone()) {
                out.push(SourcedValue::new(s, origin));
            }
        }
    }
    out
}

/// Extract catalog-number-like substrings from a single line, applying the
/// two catalog regexes and the ZIP false-positive rejection. Returns matches
/// in left-to-right order with no cross-line dedup — the caller
/// ([`catalog_numbers_sourced`]) owns dedup so it can pair survivors with
/// whatever metadata each needs.
fn find_catalogs_in_line(line: &str) -> Vec<String> {
    static MULTI_LETTER: OnceLock<Regex> = OnceLock::new();
    static SINGLE_LETTER_DIGIT: OnceLock<Regex> = OnceLock::new();
    let multi = MULTI_LETTER.get_or_init(|| Regex::new(r"\b[A-Z]{2,6}[- ]?\d{3,7}\b").unwrap());
    let single =
        SINGLE_LETTER_DIGIT.get_or_init(|| Regex::new(r"\b[A-Z]\d[- ]?\d{4,7}\b").unwrap());

    let mut out: Vec<String> = Vec::new();
    // Each regex extracts independently; the line-local order follows the
    // multi-letter regex first, then the single-letter-digit one.
    for m in multi.find_iter(line).chain(single.find_iter(line)) {
        let s = m.as_str().to_string();
        if is_zip_false_positive(&s, line) {
            continue;
        }
        out.push(s);
    }
    out
}

/// Treat a candidate as a ZIP false positive only when it matches the tail
/// state+ZIP capture (`\b<ST>\s\d{5}\b` at end of line) of a US mailing
/// address. Anything that isn't at the trailing-address position — e.g. a
/// `ZZ 12345` that happens to appear mid-line next to a `P.O. Box` — stays
/// in, because mid-line catalog-shaped strings on mail-heavy lines are
/// real catalogs often enough that we'd lose signal by dropping them.
fn is_zip_false_positive(candidate: &str, line: &str) -> bool {
    static STATE_ZIP_TAIL: OnceLock<Regex> = OnceLock::new();
    static STATE_ZIP_SHAPE: OnceLock<Regex> = OnceLock::new();

    let tail = STATE_ZIP_TAIL
        .get_or_init(|| Regex::new(r"\b([A-Z]{2})\s+(\d{5})(?:-\d{4})?\s*\.?\s*$").unwrap());
    let shape = STATE_ZIP_SHAPE.get_or_init(|| Regex::new(r"^([A-Z]{2})[- ](\d{5})$").unwrap());

    // Candidate must be state-abbrev + 5-digit shape; otherwise it can't be
    // a ZIP false positive.
    let Some(caps) = shape.captures(candidate) else {
        return false;
    };
    let candidate_state = caps.get(1).unwrap().as_str();
    let candidate_zip = caps.get(2).unwrap().as_str();

    let Some(tail_caps) = tail.captures(line) else {
        return false;
    };
    let tail_state = tail_caps.get(1).unwrap().as_str();
    let tail_zip = tail_caps.get(2).unwrap().as_str();
    tail_state == candidate_state && tail_zip == candidate_zip
}

/// Reject predicate applied uniformly to every line, regardless of source.
/// Each sub-rule is conservative — we keep false positives in order not to
/// lose real album titles.
pub fn should_reject_line(line: &str) -> bool {
    is_barcode_line(line)
        || is_catalog_line(line)
        || is_out_of_length_band(line)
        || is_track_listing(line)
        || has_runtime_suffix(line)
        || is_credit_pattern(line)
        || is_legal_prose(line)
        || is_all_digits(line)
        || is_universal_stop_phrase(line)
}

fn is_barcode_line(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^[\d\s\-]{8,16}$").unwrap());
    re.is_match(line)
}

fn is_catalog_line(line: &str) -> bool {
    static MULTI: OnceLock<Regex> = OnceLock::new();
    static SINGLE: OnceLock<Regex> = OnceLock::new();
    let multi = MULTI.get_or_init(|| Regex::new(r"^\s*[A-Z]{2,6}[- ]?\d{3,7}\s*$").unwrap());
    let single = SINGLE.get_or_init(|| Regex::new(r"^\s*[A-Z]\d[- ]?\d{4,7}\s*$").unwrap());
    multi.is_match(line) || single.is_match(line)
}

fn is_out_of_length_band(line: &str) -> bool {
    let len = line.trim().chars().count();
    !(3..=50).contains(&len)
}

/// Track-listing pattern: leading track number followed by `.` or `)`.
/// Matches `1. Title`, `02) Title`, ` 3.  Title With Leading Space`.
fn is_track_listing(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^\s*\d{1,3}[.)]\s+").unwrap());
    re.is_match(line)
}

/// Runtime parenthetical at end of line: `(5:09)`, `(10:32)`.
fn has_runtime_suffix(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\(\d{1,2}:\d{2}\)\s*$").unwrap());
    re.is_match(line)
}

/// Credit pattern: `Label:  Value` where the label is under 30 chars and
/// the value contains a role word. Captures `Engineered by Name`,
/// `Name One: bass`, `Produced by Name Two`. Two shapes:
///
/// * `<Role phrase> by <Name>` — `Produced by`, `Engineered by`, etc.
/// * `<Name>: <Role>` — `Name One: bass`, `Name Two: drums`.
fn is_credit_pattern(line: &str) -> bool {
    static COLON_RE: OnceLock<Regex> = OnceLock::new();
    static BY_RE: OnceLock<Regex> = OnceLock::new();
    let colon = COLON_RE.get_or_init(|| {
        Regex::new(
            r"(?i)^[^:]{2,30}:\s+.*\b(?:vocals?|guitar|bass|drums?|keyboards?|percussion|piano|saxophone|trumpet|produced|engineer(?:ed|ing)?|mixed|mastered|recorded|photography|photos?|design|management|artwork|writing|arranged)\b",
        )
        .unwrap()
    });
    let by = BY_RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:produced|engineered|mixed|mastered|recorded|arranged|composed|written|designed|photographed|photography|photos?|artwork|design|liner notes)\s+by\b",
        )
        .unwrap()
    });
    colon.is_match(line) || by.is_match(line)
}

/// Legal prose — copyright notices, distribution statements, mailing
/// addresses, "compact disc" handling instructions.
fn is_legal_prose(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?i)©|℗|\bunauthorized\b|\ball rights reserved\b|\bpublished by\b|\bdistributed by\b|\brecorded at\b|\bp\.?o\.?\s*box\b|\bcompact disc\b",
        )
        .unwrap()
    });
    re.is_match(line)
}

fn is_all_digits(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^[\d\s\-]+$").unwrap());
    re.is_match(line)
}

/// Lines that are *exactly* a universal label — `side a`, `cd`, `disc 1`.
/// `Side A feat. Guest Artist` is longer than the stop phrase and escapes
/// this filter naturally.
fn is_universal_stop_phrase(line: &str) -> bool {
    static STOP: OnceLock<HashSet<&'static str>> = OnceLock::new();
    let stop = STOP.get_or_init(|| {
        [
            "side a",
            "side b",
            "side 1",
            "side 2",
            "cd",
            "disc 1",
            "disc 2",
            "disc 3",
            "disc 4",
            "track",
            "album",
            "artist",
            "records",
            "tracks",
            "tracklist",
        ]
        .into_iter()
        .collect()
    });
    let normalized = line.trim().to_ascii_lowercase();
    stop.contains(normalized.as_str())
}

// ── Non-OCR extractors ──────────────────────────────────────────────────────

/// Extract `[…]` and `(…)` substrings from `folder_name` and keep only those
/// shaped like a catalog number: length 3-20, at least one alphabetic letter,
/// and at least two decimal digits. That filter lets real catalogs through
/// (`XX34b`, `LABELA071`, `Z1 12345`, `XYZ CD6`) and rejects format tags
/// (`Vinyl`, `Deluxe Edition`, `2 CD` — one digit), noise (`remaster`), and
/// bare years (`1990` — no letters).
///
/// Survivors are routed to the catalog pool directly — they bypass the
/// free-text catalog regex, which is too strict for many real-world formats.
pub fn extract_folder_brackets(folder_name: &str) -> Vec<String> {
    static BRACKET_RE: OnceLock<Regex> = OnceLock::new();
    let re = BRACKET_RE.get_or_init(|| Regex::new(r"[\[\(]([^\]\)]+)[\]\)]").unwrap());

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for caps in re.captures_iter(folder_name) {
        let inner = caps.get(1).unwrap().as_str().trim();
        if !is_catalog_shaped_bracket(inner) {
            continue;
        }
        let s = inner.to_string();
        if seen.insert(s.clone()) {
            out.push(s);
        }
    }
    out
}

fn is_catalog_shaped_bracket(s: &str) -> bool {
    let len = s.chars().count();
    if !(3..=20).contains(&len) {
        return false;
    }
    let letters = s.chars().filter(|c| c.is_ascii_alphabetic()).count();
    let digits = s.chars().filter(|c| c.is_ascii_digit()).count();
    letters >= 1 && digits >= 2
}

/// Normalize a path component for the free-text pool. Strips:
///
/// * leading year prefix (`1989 - `, `1989. `, `1989 `)
/// * leading track-number prefix (`01. `, `1) `, `1 - `)
/// * trailing bracketed / parenthesized tails (already routed via
///   `extract_folder_brackets`)
///
/// Returns `None` if the stripped component degenerates to something too
/// short to be useful (e.g. an empty string or a single character).
pub fn strip_path_component(raw: &str) -> Option<String> {
    static YEAR_PREFIX: OnceLock<Regex> = OnceLock::new();
    static TRACK_PREFIX: OnceLock<Regex> = OnceLock::new();
    static TRAILING_BRACKET: OnceLock<Regex> = OnceLock::new();

    let year = YEAR_PREFIX.get_or_init(|| Regex::new(r"^\s*\d{4}\s*[.\-)]?\s*").unwrap());
    let track = TRACK_PREFIX.get_or_init(|| Regex::new(r"^\s*\d{1,3}\s*[.\-)]\s*").unwrap());
    let bracket =
        TRAILING_BRACKET.get_or_init(|| Regex::new(r"\s*[\[\(][^\]\)]*[\]\)]\s*$").unwrap());

    let mut s = raw.trim().to_string();
    // Strip trailing brackets repeatedly — a component may have multiple
    // (`Album Title [Deluxe] (2020)`).
    loop {
        let stripped = bracket.replace(&s, "").into_owned();
        let stripped = stripped.trim_end().to_string();
        if stripped == s {
            break;
        }
        s = stripped;
    }
    // Year prefix (greedy — covers `1989 - Title` and `1989. Title` and
    // `1989 Title`). Only apply if what remains is non-empty.
    if let Some(m) = year.find(&s) {
        let rest = &s[m.end()..];
        if !rest.trim().is_empty() {
            s = rest.trim_start().to_string();
        }
    }
    // Track-number prefix.
    if let Some(m) = track.find(&s) {
        let rest = &s[m.end()..];
        if !rest.trim().is_empty() {
            s = rest.trim_start().to_string();
        }
    }
    let s = s.trim().to_string();
    if s.chars().count() < 2 {
        return None;
    }
    // All-digit remainder is noise (bare years, track-listing fragments).
    // Same rule the free-text filter applies.
    if s.chars().all(|c| c.is_ascii_digit() || c.is_whitespace()) {
        return None;
    }
    Some(s)
}

/// Extract free-text suggestions from a filename. Takes the file stem (no
/// extension), strips a leading track-number, and rejects a small set of
/// generic names (`cover`, `front`, `back`, `booklet-01`, `disc 1`, etc.).
///
/// Audio filenames are never routed here — their stems are overwhelmingly
/// track titles, which are the wrong pool for Artist / Album autocomplete.
/// See `enumerate_filename_inputs` in the service.
pub fn parse_filename_stem(path: &Path) -> Vec<String> {
    static TRACK_NUM: OnceLock<Regex> = OnceLock::new();
    static GENERIC: OnceLock<Regex> = OnceLock::new();
    let track_num = TRACK_NUM.get_or_init(|| Regex::new(r"^\s*\d{1,3}\s*[.\-_ ]+").unwrap());
    let generic = GENERIC.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:cover|front|back|booklet(?:[\s\-_]*\d+)?|disc(?:[\s\-_]*\d+)?|cd(?:[\s\-_]*\d+)?|tray|inside|inlay|card|matrix|folder|album|scan(?:[\s\-_]*\d+)?|artwork|image(?:[\s\-_]*\d+)?|thumb|thumbnail|untitled|img(?:[\s\-_]*\d+)?|dsc(?:[\s\-_]*\d+)?|photo(?:[\s\-_]*\d+)?)$",
        )
        .unwrap()
    });

    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s.trim(),
        None => return Vec::new(),
    };
    if stem.is_empty() {
        return Vec::new();
    }

    let stripped = track_num.replace(stem, "").trim().to_string();
    if stripped.is_empty() || generic.is_match(&stripped) {
        return Vec::new();
    }

    vec![stripped]
}

// ── Sourced pipeline: filter → normalize → cluster → rank → cutoff ──────────

/// Where a `SourcedLine` came from. Each variant carries the minimum
/// context needed to display or re-read the original. Used by the
/// clustering pipeline to score cluster members.
///
/// Folder-bracket extractions don't appear here — they bypass the sourced
/// pipeline entirely and flow through `bracket_catalogs: Vec<String>` on
/// the `Pool`, routed straight to the catalog output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    Artwork(PathBuf),
    PathComponent,
    FilenameGeneric(PathBuf),
    CueField,
    TextFile(PathBuf),
}

/// A candidate line tagged with its provenance. The classifier preserves the
/// original text verbatim for display; normalization is only used internally
/// for clustering.
#[derive(Debug, Clone)]
pub struct SourcedLine {
    pub source: Source,
    pub text: String,
}

/// Minimum normalized length before a line is eligible for clustering.
/// Shorter strings make Jaro-Winkler similarity unreliable.
const MIN_CLUSTER_LEN: usize = 4;

/// Jaro-Winkler similarity floor for cluster membership.
const JW_THRESHOLD: f64 = 0.85;

/// Upper bound on the free-text pool size. Starting value — tune against the
/// corpus.
pub const FREE_TEXT_CAP: usize = 30;

/// Baseline free-text cutoff: clusters with at least this score survive.
pub const FREE_TEXT_MIN_SCORE: usize = 2;

/// Fallback free-text cutoff: when no cluster clears the min-score gate
/// (e.g. single-image candidate), take the top N by cluster size instead of
/// returning an empty pool.
pub const FREE_TEXT_FALLBACK_TOP: usize = 15;

/// One cluster of sourced lines grouped by normalized-text similarity.
#[derive(Debug, Clone)]
pub struct Cluster {
    pub normalized_centroid: String,
    pub members: Vec<SourcedLine>,
}

impl Cluster {
    /// Cluster score: Σ (source_weight × members-of-that-source). The
    /// weights are eyeballed starting values — tune against corpus.
    pub fn score(&self) -> usize {
        self.members.iter().map(|m| source_weight(&m.source)).sum()
    }

    /// Pick a display form for the cluster. Preference order:
    ///
    /// 1. Highest source weight (CUE > path > audio-split > generic / OCR).
    /// 2. Title-case over ALL-CAPS over all-lowercase.
    /// 3. Longest text (OCR variants of the same name lose characters when
    ///    garbled; real text tends to be longer than truncated renderings).
    ///
    /// Members shorter than 4 chars are skipped — prevents truncated OCR
    /// fragments from winning. If every member is below that floor (rare —
    /// happens only for tiny candidate strings), take the first.
    pub fn pick_representative(&self) -> String {
        self.members
            .iter()
            .enumerate()
            .filter_map(|(index, m)| {
                let len = m.text.chars().count();
                if len < 4 {
                    return None;
                }
                Some((
                    source_weight(&m.source),
                    case_rank(&m.text),
                    len,
                    std::cmp::Reverse(index),
                    m,
                ))
            })
            .max_by_key(|(weight, case_rank, len, index, _)| (*weight, *case_rank, *len, *index))
            .map(|(_, _, _, _, m)| m.text.clone())
            .or_else(|| self.members.first().map(|m| m.text.clone()))
            .unwrap_or_default()
    }
}

/// Source-weight starting values. CUE and path components are curated by
/// rippers / users so they carry the strongest per-line signal; artwork OCR
/// is lower weight per-line but earns score by repeating across images.
pub fn source_weight(source: &Source) -> usize {
    match source {
        Source::CueField => 5,
        Source::PathComponent => 3,
        Source::FilenameGeneric(_) => 1,
        Source::Artwork(_) => 1,
        Source::TextFile(_) => 1,
    }
}

/// Case-style rank for cluster-representative selection: titled / mixed
/// wins over ALL-CAPS over all-lowercase. Tie-break only.
fn case_rank(s: &str) -> u8 {
    let has_upper = s.chars().any(|c| c.is_uppercase());
    let has_lower = s.chars().any(|c| c.is_lowercase());
    match (has_upper, has_lower) {
        (true, true) => 2,   // Title / Mixed
        (true, false) => 1,  // ALL-CAPS
        (false, true) => 0,  // all lowercase
        (false, false) => 0, // no letters — treat as lowercase for tie-break
    }
}

/// Normalize text for similarity comparison. Preserves the original for
/// display; this output is only used as the clustering key.
///
/// Pipeline: NFD decomposition → drop combining marks (strips diacritics)
/// → lowercase → trim → collapse internal whitespace → strip leading /
/// trailing non-alphanumerics.
pub fn normalize(text: &str) -> String {
    // NFD decompose, drop combining marks.
    let decomposed: String = text
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect();
    let mut s = decomposed.to_lowercase();
    // Collapse any run of whitespace to a single space.
    let mut collapsed = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                collapsed.push(' ');
                prev_space = true;
            }
        } else {
            collapsed.push(c);
            prev_space = false;
        }
    }
    s = collapsed.trim().to_string();
    // Strip leading / trailing non-alphanumeric runs.
    while s
        .chars()
        .next()
        .map(|c| !c.is_alphanumeric())
        .unwrap_or(false)
    {
        s.remove(0);
    }
    while s
        .chars()
        .last()
        .map(|c| !c.is_alphanumeric())
        .unwrap_or(false)
    {
        s.pop();
    }
    s
}

/// Greedy incremental clustering. Each line joins the closest existing
/// cluster whose Jaro-Winkler similarity ≥ `JW_THRESHOLD`, otherwise starts
/// its own. Lines below `MIN_CLUSTER_LEN` after normalization form their
/// own singleton clusters (similarity on short strings is unreliable).
///
/// Convenience wrapper over `cluster_lines_incremental` — starts from an
/// empty cluster set. Used by tests and by one-shot classify paths.
pub fn cluster_lines(lines: Vec<SourcedLine>) -> Vec<Cluster> {
    let mut clusters: Vec<Cluster> = Vec::new();
    cluster_lines_incremental(&mut clusters, &lines);
    clusters
}

/// Classify `new_lines` against `clusters`, appending each line into the
/// best-matching cluster or starting a new one. Mutates `clusters` in place
/// so the caller can hold it across calls and avoid re-classifying old
/// lines each emission.
pub fn cluster_lines_incremental(clusters: &mut Vec<Cluster>, new_lines: &[SourcedLine]) {
    for line in new_lines {
        let norm = normalize(&line.text);
        if norm.is_empty() {
            continue;
        }
        if norm.chars().count() < MIN_CLUSTER_LEN {
            // Short lines still go into the output as singleton clusters —
            // otherwise we'd lose real 2-3 char album titles.
            clusters.push(Cluster {
                normalized_centroid: norm,
                members: vec![line.clone()],
            });
            continue;
        }

        // Find the most-similar existing cluster.
        let mut best_idx: Option<usize> = None;
        let mut best_sim: f64 = 0.0;
        for (i, c) in clusters.iter().enumerate() {
            let sim = jaro_winkler(&c.normalized_centroid, &norm);
            if sim > best_sim {
                best_sim = sim;
                best_idx = Some(i);
            }
        }

        if let Some(i) = best_idx {
            if best_sim >= JW_THRESHOLD {
                clusters[i].members.push(line.clone());
                continue;
            }
        }
        clusters.push(Cluster {
            normalized_centroid: norm,
            members: vec![line.clone()],
        });
    }
}

/// Sort clusters by score descending.
pub fn rank_clusters(mut clusters: Vec<Cluster>) -> Vec<Cluster> {
    rank_clusters_in_place(&mut clusters);
    clusters
}

/// In-place rank. Same sort key as `rank_clusters`, for callers that hold
/// an owned `Vec` and want to avoid the move/return dance.
pub fn rank_clusters_in_place(clusters: &mut [Cluster]) {
    clusters.sort_by_key(|c| std::cmp::Reverse(c.score()));
}

/// Apply the free-text cutoff and turn the surviving clusters into display
/// strings.
///
/// Primary rule: clusters with `score ≥ FREE_TEXT_MIN_SCORE` survive, capped
/// at `FREE_TEXT_CAP`. When no cluster clears the min-score gate (common on
/// single-image candidates where every cluster is score-1), fall back to
/// the top `FREE_TEXT_FALLBACK_TOP` by score — the UI still gets a useful
/// dropdown instead of an empty one.
pub fn apply_free_text_cutoff(ranked: &[Cluster]) -> Vec<String> {
    let above_threshold: Vec<&Cluster> = ranked
        .iter()
        .filter(|c| c.score() >= FREE_TEXT_MIN_SCORE)
        .collect();

    if !above_threshold.is_empty() {
        return above_threshold
            .into_iter()
            .take(FREE_TEXT_CAP)
            .map(|c| c.pick_representative())
            .filter(|s| !s.is_empty())
            .collect();
    }

    ranked
        .iter()
        .take(FREE_TEXT_FALLBACK_TOP)
        .map(|c| c.pick_representative())
        .filter(|s| !s.is_empty())
        .collect()
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the live sourced catalog extractor over plain lines and project to
    /// bare values. Production reaches `find_catalogs_in_line`, ZIP rejection,
    /// and dedup through `catalog_numbers_sourced`; these assertions pin that
    /// shared logic through the real entry point.
    fn cats(lines: &[String]) -> Vec<String> {
        let sourced: Vec<SourcedLine> = lines
            .iter()
            .map(|t| SourcedLine {
                source: Source::CueField,
                text: t.clone(),
            })
            .collect();
        catalog_numbers_sourced(&sourced)
            .into_iter()
            .map(|c| c.value)
            .collect()
    }

    /// Lines that survive `should_reject_line` — the free-text pool's reject
    /// pass, exercised here line by line. Production applies the same predicate
    /// before clustering.
    fn kept_lines(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .filter(|l| !should_reject_line(l))
            .cloned()
            .collect()
    }

    // MARK: - catalog_numbers — preserve separators verbatim

    #[test]
    fn catalogs_preserves_hyphen() {
        // hyphen catalog is preserved verbatim
        assert_eq!(
            cats(&["WPCR-80001".to_string()]),
            vec!["WPCR-80001".to_string()],
        );
    }

    #[test]
    fn catalogs_preserves_inner_space() {
        // inner space catalog is preserved verbatim
        assert_eq!(
            cats(&["COCQ 84487".to_string()]),
            vec!["COCQ 84487".to_string()],
        );
    }

    #[test]
    fn catalogs_substring_match() {
        // catalog substring inside a larger line
        assert_eq!(
            cats(&["Part No. TOCP12345".to_string()]),
            vec!["TOCP12345".to_string()],
        );
    }

    #[test]
    fn catalogs_lowercase_rejected() {
        // lowercase is rejected — catalogs require uppercase letters
        let empty: Vec<String> = Vec::new();
        assert_eq!(cats(&["lowercase-12345".to_string()]), empty);
    }

    #[test]
    fn catalogs_separator_variants_are_distinct() {
        // hyphen and space variants of the same prefix are distinct tokens
        assert_eq!(
            cats(&["WPCR-80001".to_string(), "WPCR 80001".to_string()]),
            vec!["WPCR-80001".to_string(), "WPCR 80001".to_string()],
        );
    }

    // MARK: - catalog_numbers — multi-disc suffix behavior
    //
    // These assertions pin current behavior of the catalog regex
    // (`\b[A-Z]{2,6}[- ]?\d{3,7}\b`) against multi-disc catalog numbers. The
    // `\b` word boundaries end the match at the first non-word character, so
    // suffixes after `/` or `~` fall outside the match. Formats with two
    // internal separators (e.g. `UDCD-1-702`) don't match at all — the regex
    // permits only one separator between letters and digits.

    #[test]
    fn catalogs_multi_disc_slash_suffix() {
        // multi-disc slash suffix — the first disc's catalog number is returned, suffix dropped
        assert_eq!(
            cats(&["BVCK-15024/5".to_string()]),
            vec!["BVCK-15024".to_string()],
        );
    }

    #[test]
    fn catalogs_multi_disc_tilde_suffix() {
        // multi-disc tilde suffix — the first disc's catalog number is returned, suffix dropped
        assert_eq!(
            cats(&["BVCP 21011~2".to_string()]),
            vec!["BVCP 21011".to_string()],
        );
    }

    #[test]
    fn catalogs_two_internal_separators_rejected() {
        // MFSL-style two-separator catalog is not matched
        let empty: Vec<String> = Vec::new();
        assert_eq!(cats(&["MFSL UDCD-1-702".to_string()]), empty);
    }

    // MARK: - catalog_numbers_sourced — origins + parity with catalog_numbers

    #[test]
    fn sourced_catalogs_attribute_origin_per_line() {
        // A folder-bracket-shaped catalog can't reach here (brackets bypass
        // this path); use a path component and an artwork line. Each survivor
        // carries the SignalOrigin its source projects to.
        let lines = vec![
            SourcedLine {
                source: Source::PathComponent,
                text: "WPCR-80001".to_string(),
            },
            SourcedLine {
                source: Source::Artwork(PathBuf::from("/cover.jpg")),
                text: "COCQ 84487".to_string(),
            },
        ];
        let out = catalog_numbers_sourced(&lines);
        assert_eq!(
            out,
            vec![
                SourcedValue::new("WPCR-80001".to_string(), SignalOrigin::FolderName),
                SourcedValue::new("COCQ 84487".to_string(), SignalOrigin::Artwork),
            ],
        );
    }

    #[test]
    fn sourced_catalogs_reject_zip_and_dedup() {
        // The sourced extractor applies the catalog regexes, rejects the
        // mailing-address ZIP tail, and dedupes by first-seen order.
        let raw = vec![
            "WPCR-80001".to_string(),
            "Some City, NY 10001".to_string(), // ZIP tail — rejected
            "WPCR-80001".to_string(),          // duplicate — dropped
            "Z1 12345".to_string(),
        ];
        assert_eq!(
            cats(&raw),
            vec!["WPCR-80001".to_string(), "Z1 12345".to_string()]
        );
    }

    // MARK: - free_text

    #[test]
    fn free_text_drops_catalog_lines() {
        // whole-line catalog is filtered out of free text
        assert_eq!(
            kept_lines(&["Album Title".to_string(), "WPCR-80001".to_string()]),
            vec!["Album Title".to_string()],
        );
    }

    #[test]
    fn free_text_keeps_substring_catalog_lines() {
        // substring catalog keeps the surrounding free-text line
        assert_eq!(
            kept_lines(&["Label Records - WPCR-80001".to_string()]),
            vec!["Label Records - WPCR-80001".to_string()],
        );
    }

    // MARK: - sanity

    #[test]
    fn catalogs_empty_input() {
        // sanity: empty input yields empty output
        let empty: Vec<String> = Vec::new();
        assert_eq!(cats(&[]), empty);
    }

    // MARK: - extract_folder_brackets

    #[test]
    fn brackets_accepts_catalog_shapes() {
        // Mix of real-world catalog shapes — letter+digit combinations of
        // varying lengths, with and without separators.
        let folder = "Album Title [XX34b] (LABELA071) [Z1 12345]";
        let out = extract_folder_brackets(folder);
        assert_eq!(
            out,
            vec![
                "XX34b".to_string(),
                "LABELA071".to_string(),
                "Z1 12345".to_string(),
            ],
        );
    }

    #[test]
    fn brackets_rejects_format_tags() {
        // Format tags (`Vinyl`, `Deluxe Edition`) have letters but no digits
        // or only one digit; both fail the ≥2-digit filter.
        let folder = "Album Title [Vinyl] (Deluxe Edition) [2 CD]";
        let out = extract_folder_brackets(folder);
        assert!(out.is_empty(), "expected empty, got {out:?}");
    }

    #[test]
    fn brackets_rejects_bare_year() {
        // `1990` has digits but no letters — fails the ≥1-letter filter.
        let folder = "Album Title (1990)";
        assert!(extract_folder_brackets(folder).is_empty());
    }

    #[test]
    fn brackets_rejects_over_length() {
        // Anything longer than 20 chars bails out — keeps long quotations,
        // liner-note fragments, etc. out of the catalog pool.
        let folder = "Album [This is way too long to be a catalog]";
        assert!(extract_folder_brackets(folder).is_empty());
    }

    // MARK: - strip_path_component

    #[test]
    fn path_strips_year_and_bracket() {
        assert_eq!(
            strip_path_component("1989 - Album Title [XX34b]"),
            Some("Album Title".to_string()),
        );
    }

    #[test]
    fn path_strips_dotted_year() {
        assert_eq!(
            strip_path_component("1989. Album Title"),
            Some("Album Title".to_string()),
        );
    }

    #[test]
    fn path_strips_trailing_brackets_multiple() {
        assert_eq!(
            strip_path_component("Album Title [Deluxe] (2020)"),
            Some("Album Title".to_string()),
        );
    }

    #[test]
    fn path_leaves_leading_bracket_alone() {
        // TRAILING_BRACKET is anchored at end-of-line, so a leading bracket
        // stays attached to the rest of the component.
        assert_eq!(
            strip_path_component("[Remaster] Album Title"),
            Some("[Remaster] Album Title".to_string()),
        );
    }

    #[test]
    fn path_iteratively_strips_multiple_trailing_brackets() {
        // Two trailing brackets come off in successive loop iterations.
        assert_eq!(
            strip_path_component("Album Title [Remaster] [Deluxe]"),
            Some("Album Title".to_string()),
        );
    }

    #[test]
    fn path_returns_none_when_empty() {
        // All-bracket content degenerates to empty.
        assert_eq!(strip_path_component("[XX34b]"), None);
        assert_eq!(strip_path_component("1989"), None);
    }

    #[test]
    fn path_min_length_gate() {
        // After stripping, a 1-char remainder is rejected as uninteresting.
        assert_eq!(strip_path_component("1989 - X"), None);
        // 2 chars survives — edge case but harmless downstream.
        assert_eq!(strip_path_component("1989 - AB"), Some("AB".to_string()));
    }

    // MARK: - parse_filename_stem

    #[test]
    fn filename_returns_stripped_stem() {
        assert_eq!(
            parse_filename_stem(Path::new("/m/Artist Name - Album Title.png")),
            vec!["Artist Name - Album Title".to_string()],
        );
    }

    #[test]
    fn filename_strips_track_number() {
        // Track-number prefix dropped; the rest kept as a single stem.
        assert_eq!(
            parse_filename_stem(Path::new("/m/01 - Back Cover.png")),
            vec!["Back Cover".to_string()],
        );
    }

    #[test]
    fn filename_rejects_generic_names() {
        // `cover.jpg` — entirely generic.
        assert!(parse_filename_stem(Path::new("/m/cover.jpg")).is_empty());
        assert!(parse_filename_stem(Path::new("/m/Booklet-01.png")).is_empty());
        assert!(parse_filename_stem(Path::new("/m/disc 1.jpg")).is_empty());
    }

    #[test]
    fn filename_keeps_non_generic_image() {
        assert_eq!(
            parse_filename_stem(Path::new("/m/Artist Name - Album.png")),
            vec!["Artist Name - Album".to_string()],
        );
    }

    // MARK: - catalog_numbers — single-letter-digit prefix (Phase 3)

    #[test]
    fn catalogs_single_letter_digit_prefix() {
        // `Z1 12345`, `T5 67890` — single letter + single digit, then ≥4 digits.
        assert_eq!(
            cats(&["Z1 12345".to_string()]),
            vec!["Z1 12345".to_string()],
        );
        assert_eq!(
            cats(&["T5-67890".to_string()]),
            vec!["T5-67890".to_string()],
        );
    }

    #[test]
    fn catalogs_substring_single_letter_digit() {
        // Embedded single-letter-digit catalog.
        assert_eq!(
            cats(&["Released on Z1 12345 in 2020".to_string()]),
            vec!["Z1 12345".to_string()],
        );
    }

    // MARK: - catalog_numbers — ZIP false positives (Phase 3)

    #[test]
    fn catalogs_zip_in_po_box_rejected() {
        // Classic mailing address: state+ZIP at end of line gets stripped,
        // regardless of whether a PO Box marker is present.
        let empty: Vec<String> = Vec::new();
        assert_eq!(cats(&["P.O. Box 123, City, ZZ 12345.".to_string()]), empty,);
        assert_eq!(
            cats(&["PO Box 4567, Other City, XX 98765".to_string()]),
            empty,
        );
    }

    #[test]
    fn catalogs_zip_tail_rejected() {
        // State abbrev + 5 digits at end of line — mailing-address tail.
        let empty: Vec<String> = Vec::new();
        assert_eq!(cats(&["Some City, NY 10001".to_string()]), empty,);
    }

    #[test]
    fn catalogs_midline_catalog_on_mail_line_kept() {
        // A line that contains a `P.O. Box` but whose catalog-shaped
        // substring is mid-line (not at the trailing address position)
        // must keep the catalog. Only the end-of-line state+ZIP capture
        // gets dropped.
        assert_eq!(
            cats(&["P.O. Box 123 — TX 45678 is the album code".to_string()]),
            vec!["TX 45678".to_string()],
        );
    }

    #[test]
    fn catalogs_real_catalog_beside_zip_kept() {
        // Real catalog on the same line as a ZIP-shaped string should still
        // survive. Only the ZIP-shaped candidate gets dropped.
        assert_eq!(
            cats(&["WPCR-80001 / City, NY 10001".to_string()]),
            vec!["WPCR-80001".to_string()],
        );
    }

    // MARK: - free_text — line-level filters (Phase 3)

    #[test]
    fn free_text_rejects_track_listing() {
        assert!(kept_lines(&["1. Track Title".to_string()]).is_empty());
        assert!(kept_lines(&["  02) Track Title".to_string()]).is_empty());
        assert!(kept_lines(&[" 3.  Track Title With Spaces".to_string()]).is_empty());
    }

    #[test]
    fn free_text_rejects_runtime_suffix() {
        assert!(kept_lines(&["Track Title (5:09)".to_string()]).is_empty());
        assert!(kept_lines(&["Something else (10:32) ".to_string()]).is_empty());
    }

    #[test]
    fn free_text_rejects_credit_patterns() {
        assert!(kept_lines(&["Name One: bass".to_string()]).is_empty());
        assert!(kept_lines(&["Name Two: drums".to_string()]).is_empty());
        assert!(kept_lines(&["Engineered by Name Three".to_string()]).is_empty());
        assert!(kept_lines(&["Produced by Name Four".to_string()]).is_empty());
        assert!(kept_lines(&["Photography by Name Five".to_string()]).is_empty());
    }

    #[test]
    fn free_text_rejects_legal_prose() {
        assert!(
            kept_lines(&["\u{00A9} 2020 Label Name. All rights reserved.".to_string()]).is_empty()
        );
        assert!(kept_lines(&["Published by Publisher Name".to_string()]).is_empty());
        assert!(kept_lines(&["Distributed by Distributor".to_string()]).is_empty());
        assert!(kept_lines(&["P.O. Box 123, City, ZZ 12345.".to_string()]).is_empty());
        assert!(kept_lines(&["Compact Disc Digital Audio".to_string()]).is_empty());
    }

    #[test]
    fn free_text_rejects_out_of_length_band() {
        // Too short.
        assert!(kept_lines(&["A".to_string(), "OK".to_string()]).is_empty());
        // Too long (>50 chars).
        let long = "This is a very long liner-note sentence that exceeds fifty characters handily."
            .to_string();
        assert!(kept_lines(&[long]).is_empty());
    }

    #[test]
    fn free_text_rejects_all_digit_and_stop_phrases() {
        assert!(kept_lines(&["  1234-5678  ".to_string()]).is_empty());
        assert!(kept_lines(&["Side A".to_string()]).is_empty());
        assert!(kept_lines(&["disc 1".to_string()]).is_empty());
        assert!(kept_lines(&["CD".to_string()]).is_empty());
    }

    #[test]
    fn free_text_keeps_real_title_with_stop_prefix() {
        // `Side A feat. Guest Artist` isn't exactly `side a` — keep it.
        assert_eq!(
            kept_lines(&["Side A feat. Guest Artist".to_string()]),
            vec!["Side A feat. Guest Artist".to_string()],
        );
    }

    #[test]
    fn free_text_keeps_titles_with_colons_outside_credits() {
        // Title with a colon but no role keyword — not a credit pattern.
        assert_eq!(
            kept_lines(&["Album Title: The Subtitle".to_string()]),
            vec!["Album Title: The Subtitle".to_string()],
        );
    }

    // MARK: - normalize (Phase 4)

    #[test]
    fn normalize_strips_diacritics() {
        assert_eq!(normalize("Café"), "cafe");
        assert_eq!(normalize("Fjörn"), "fjorn");
    }

    #[test]
    fn normalize_lowercases_and_collapses_whitespace() {
        assert_eq!(normalize("  Album   Title  "), "album title");
        assert_eq!(normalize("Album\tTitle"), "album title");
    }

    #[test]
    fn normalize_strips_leading_trailing_nonalnum() {
        assert_eq!(normalize("\"Album Title\""), "album title");
        assert_eq!(normalize("!!!Album!!!"), "album");
    }

    // MARK: - cluster_lines

    fn artwork_line(path: &str, text: &str) -> SourcedLine {
        SourcedLine {
            source: Source::Artwork(PathBuf::from(path)),
            text: text.to_string(),
        }
    }

    fn path_line(text: &str) -> SourcedLine {
        SourcedLine {
            source: Source::PathComponent,
            text: text.to_string(),
        }
    }

    fn cue_line(text: &str) -> SourcedLine {
        SourcedLine {
            source: Source::CueField,
            text: text.to_string(),
        }
    }

    #[test]
    fn cluster_groups_ocr_variants_together() {
        // Three spellings of the same name — clean + two garbled.
        let lines = vec![
            artwork_line("/a.jpg", "Artist Alpha"),
            artwork_line("/b.jpg", "ARTST ALPA"), // char dropped
            artwork_line("/c.jpg", "Arist Alpha"), // char dropped
            artwork_line("/d.jpg", "Totally Different"),
        ];
        let clusters = cluster_lines(lines);
        // At least the three "Artist Alpha" variants should cluster. We
        // don't pin exact counts — just verify grouping behavior.
        let alpha_cluster = clusters
            .iter()
            .find(|c| c.members.len() >= 2)
            .expect("expected a multi-member cluster for OCR variants");
        assert!(
            alpha_cluster.members.len() >= 3,
            "expected 3+ variant cluster, got {:?}",
            alpha_cluster.members,
        );
    }

    #[test]
    fn cluster_keeps_distinct_names_separate() {
        let lines = vec![
            path_line("Artist Alpha"),
            path_line("Album Title B"),
            cue_line("Name Two"),
        ];
        let clusters = cluster_lines(lines);
        assert_eq!(clusters.len(), 3);
        for c in &clusters {
            assert_eq!(c.members.len(), 1);
        }
    }

    /// Incremental clustering: two `cluster_lines_incremental` calls
    /// produce the same cluster topology as a single all-at-once call.
    /// This is what the service relies on for O(n) per-emission cost.
    #[test]
    fn cluster_incremental_matches_whole_pool() {
        let first = vec![
            path_line("Artist Alpha"),
            artwork_line("/a.jpg", "Album Title B"),
        ];
        let second = vec![
            artwork_line("/b.jpg", "Arist Alpha"),
            artwork_line("/c.jpg", "Album Title B"),
            cue_line("Totally Different"),
        ];

        let mut incremental: Vec<Cluster> = Vec::new();
        cluster_lines_incremental(&mut incremental, &first);
        cluster_lines_incremental(&mut incremental, &second);

        let combined: Vec<SourcedLine> = first.iter().chain(second.iter()).cloned().collect();
        let all_at_once = cluster_lines(combined);

        assert_eq!(incremental.len(), all_at_once.len());
        for (a, b) in incremental.iter().zip(all_at_once.iter()) {
            assert_eq!(a.normalized_centroid, b.normalized_centroid);
            assert_eq!(a.members.len(), b.members.len());
        }
    }

    // MARK: - source_weight + cluster scoring

    #[test]
    fn cluster_score_combines_source_weights() {
        let cluster = Cluster {
            normalized_centroid: "artist alpha".to_string(),
            members: vec![
                cue_line("Artist Alpha"),               // 5
                path_line("Artist Alpha"),              // 3
                artwork_line("/a.jpg", "Artist Alpha"), // 1
            ],
        };
        assert_eq!(cluster.score(), 9);
    }

    // MARK: - pick_representative

    #[test]
    fn pick_representative_prefers_highest_source_weight() {
        let cluster = Cluster {
            normalized_centroid: "album title a".to_string(),
            members: vec![
                artwork_line("/a.jpg", "ALBM TITL A"), // garbled, weight 1
                cue_line("Album Title A"),             // clean, weight 5
            ],
        };
        assert_eq!(cluster.pick_representative(), "Album Title A");
    }

    #[test]
    fn pick_representative_prefers_mixed_case_over_all_caps() {
        let cluster = Cluster {
            normalized_centroid: "album title".to_string(),
            members: vec![
                artwork_line("/a.jpg", "ALBUM TITLE"),
                artwork_line("/b.jpg", "Album Title"),
            ],
        };
        assert_eq!(cluster.pick_representative(), "Album Title");
    }

    #[test]
    fn pick_representative_enforces_min_length() {
        // Very short OCR fragment should not win even if equal weight.
        let cluster = Cluster {
            normalized_centroid: "abc".to_string(),
            members: vec![
                artwork_line("/a.jpg", "Abc"),           // 3 chars — below 4-char floor
                artwork_line("/b.jpg", "Album Title X"), // 13 chars
            ],
        };
        assert_eq!(cluster.pick_representative(), "Album Title X");
    }

    #[test]
    fn pick_representative_falls_back_to_first_when_all_below_floor() {
        // Every member is below the 4-char floor, so the ranking loop skips
        // them all; the fallback must return the first member, not empty.
        let cluster = Cluster {
            normalized_centroid: "ab".to_string(),
            members: vec![cue_line("ab"), cue_line("xy")],
        };
        assert_eq!(cluster.pick_representative(), "ab");
    }

    #[test]
    fn apply_free_text_cutoff_gates_on_min_score_then_falls_back() {
        let make = |centroid: &str, member| Cluster {
            normalized_centroid: centroid.to_string(),
            members: vec![member],
        };

        // A CUE line scores 5 (>= FREE_TEXT_MIN_SCORE = 2); a lone artwork line
        // scores 1 (below). When something clears the gate, only the gated
        // survivors are returned — the weak cluster is dropped.
        let gated = apply_free_text_cutoff(&[
            make("strong", cue_line("Strong Title")),
            make("weak", artwork_line("/a.jpg", "Weak Title")),
        ]);
        assert_eq!(gated, vec!["Strong Title".to_string()]);

        // When nothing clears the gate, fall back to the ranked list rather
        // than returning an empty dropdown.
        let fallback =
            apply_free_text_cutoff(&[make("weak", artwork_line("/a.jpg", "Weak Title"))]);
        assert_eq!(fallback, vec!["Weak Title".to_string()]);
    }

    /// The plan's example `ARTIST NAME/ALBUM TITLE` vs `ALBUM TITLE` — the
    /// earlier text argued we'd want "shortest wins" to drop the prefix.
    /// In practice the two strings normalize to forms with Jaro-Winkler
    /// similarity below the 0.85 clustering threshold, so they never end
    /// up in the same cluster and the tie-break rule is moot. Pin that
    /// assumption so we notice if thresholds or normalization change.
    #[test]
    fn representative_prefix_and_title_land_in_different_clusters() {
        let prefix = normalize("ARTIST NAME/ALBUM TITLE");
        let plain = normalize("ALBUM TITLE");
        let sim = jaro_winkler(&prefix, &plain);
        assert!(
            sim < JW_THRESHOLD,
            "normalized JW({prefix:?}, {plain:?}) = {sim}, expected < {JW_THRESHOLD}",
        );

        // Verify via the full clustering pipeline: two separate clusters,
        // each with exactly one member.
        let lines = vec![
            artwork_line("/a.jpg", "ARTIST NAME/ALBUM TITLE"),
            artwork_line("/b.jpg", "ALBUM TITLE"),
        ];
        let clusters = cluster_lines(lines);
        assert_eq!(
            clusters.len(),
            2,
            "expected two clusters for prefix vs plain, got {clusters:?}",
        );
    }

    /// Longest-wins is the intended tie-break: when the same name is
    /// clustered together with an OCR-truncated variant, the clean full
    /// form should win over the truncation. Both members share the same
    /// source weight and case rank, so length is the deciding field.
    #[test]
    fn representative_longest_wins_on_ocr_truncation() {
        let cluster = Cluster {
            normalized_centroid: "album title".to_string(),
            members: vec![
                artwork_line("/a.jpg", "Album Titl"), // truncated by 1 char
                artwork_line("/b.jpg", "Album Title"),
            ],
        };
        assert_eq!(cluster.pick_representative(), "Album Title");
    }
}
