//! Classification of candidate text into per-field suggestion pools.
//!
//! Input is text lines harvested from one candidate's surfaces — artwork OCR is
//! the noisiest, but path components, folder-name brackets, filenames, CUE
//! sheets, and `.txt` content all feed in. Pure: no OCR, no I/O, just string
//! transforms.
//!
//! Two pools feed the import search UI:
//!
//! * catalog numbers — regex-extracted substrings, via `catalog_numbers_sourced`.
//! * free text — Artist / Album autocomplete lines, with whole-line barcodes and
//!   catalog numbers dropped by `should_reject_line` before clustering.
//!
//! Extractors for non-OCR sources:
//!
//! * `extract_folder_brackets` — bracketed substrings from folder names, kept
//!   only when catalog-shaped. Routes straight to the catalog pool, bypassing the
//!   free-text catalog regex, which is too strict for real-world formats like
//!   `Z1 12345` or `XYZ CD6`.
//! * `strip_path_component` — strips year prefixes, track-number prefixes, and
//!   trailing bracketed tails from a path segment.
//! * `parse_filename_stem` — file stem, minus extension and leading track number.

use crate::signals::{SignalOrigin, SourcedValue};
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use strsim::jaro_winkler;
use unicode_normalization::UnicodeNormalization;

/// Extract catalog-number-like substrings, each tagged with its line's
/// [`SignalOrigin`] (the Refine badges show where a candidate came from). Two
/// regexes cover most real-world formats:
///
/// * Letter prefix (`\b[A-Z]{2,6}[- ]?\d{3,7}\b`): `WPCR-80001`, `COCQ 84487`,
///   `TOCP12345`, `RR-500`.
/// * Single letter + digit (`\b[A-Z]\d[- ]?\d{4,7}\b`): `Z1 12345`, `T5 67890`.
///   Its digit suffix must be ≥4 long so short incidentals don't match.
///
/// Inner separators are preserved verbatim: MusicBrainz indexes `WPCR-80001` and
/// `WPCR 80001` as distinct tokens, so both forms survive when both appear. ZIP
/// false positives are rejected. Dedupes by first-seen order.
pub(crate) fn catalog_numbers_sourced(lines: &[SourcedLine]) -> Vec<SourcedValue> {
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

/// Catalog-number-like substrings in one line, ZIP false positives rejected. No
/// dedup — [`catalog_numbers_sourced`] owns that, across lines.
fn find_catalogs_in_line(line: &str) -> Vec<String> {
    static MULTI_LETTER: OnceLock<Regex> = OnceLock::new();
    static SINGLE_LETTER_DIGIT: OnceLock<Regex> = OnceLock::new();
    let multi = MULTI_LETTER.get_or_init(|| Regex::new(r"\b[A-Z]{2,6}[- ]?\d{3,7}\b").unwrap());
    let single =
        SINGLE_LETTER_DIGIT.get_or_init(|| Regex::new(r"\b[A-Z]\d[- ]?\d{4,7}\b").unwrap());

    // Order is all multi-letter matches, then all single-letter-digit ones —
    // the two regexes run independently, not interleaved by position.
    let mut out: Vec<String> = Vec::new();
    for m in multi.find_iter(line).chain(single.find_iter(line)) {
        let s = m.as_str().to_string();
        if is_zip_false_positive(&s, line) {
            continue;
        }
        out.push(s);
    }
    out
}

/// A candidate is a ZIP false positive only if it *is* the state+ZIP at the end
/// of the line — the tail of a US mailing address. A catalog-shaped string
/// mid-line, even next to a `P.O. Box`, stays: those are real catalogs often
/// enough that dropping them would lose signal.
fn is_zip_false_positive(candidate: &str, line: &str) -> bool {
    static STATE_ZIP_TAIL: OnceLock<Regex> = OnceLock::new();
    static STATE_ZIP_SHAPE: OnceLock<Regex> = OnceLock::new();

    let tail = STATE_ZIP_TAIL
        .get_or_init(|| Regex::new(r"\b([A-Z]{2})\s+(\d{5})(?:-\d{4})?\s*\.?\s*$").unwrap());
    let shape = STATE_ZIP_SHAPE.get_or_init(|| Regex::new(r"^([A-Z]{2})[- ](\d{5})$").unwrap());

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

/// Free-text reject predicate, applied to every line whatever its source. Each
/// sub-rule is conservative: keep false positives rather than lose real titles.
pub(crate) fn should_reject_line(line: &str) -> bool {
    is_catalog_line(line)
        || is_out_of_length_band(line)
        || is_track_listing(line)
        || has_runtime_suffix(line)
        || is_credit_pattern(line)
        || is_legal_prose(line)
        || is_all_digits(line)
        || is_universal_stop_phrase(line)
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

/// Track listing: leading track number then `.` or `)` — `1. Title`, `02) Title`.
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

/// A credit line, in either shape: `<Name>: <Role>` (`Name One: bass`, name under
/// 30 chars) or `<Role> by <Name>` (`Produced by Name Two`).
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

/// Lines that are *exactly* a universal label — `side a`, `cd`, `disc 1`. An
/// exact match, so `Side A feat. Guest Artist` escapes the filter naturally.
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

/// `[…]` and `(…)` substrings of `folder_name` that are catalog-shaped: 3-20
/// chars, ≥1 letter, ≥2 digits. That admits real catalogs (`XX34b`, `LABELA071`,
/// `Z1 12345`, `XYZ CD6`) and rejects format tags (`Vinyl`, `2 CD`), noise
/// (`remaster`), and bare years (`1990`).
///
/// Survivors go straight to the catalog pool, bypassing the free-text catalog
/// regex, which is too strict for many real-world formats.
pub(crate) fn extract_folder_brackets(folder_name: &str) -> Vec<String> {
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

/// Normalize a path component for the free-text pool: strip a leading year
/// (`1989 - `) or track number (`01. `), and trailing bracketed tails (which
/// `extract_folder_brackets` already routed to the catalog pool).
///
/// `None` when what's left is too short (under 2 chars) or all digits.
pub(crate) fn strip_path_component(raw: &str) -> Option<String> {
    static YEAR_PREFIX: OnceLock<Regex> = OnceLock::new();
    static TRACK_PREFIX: OnceLock<Regex> = OnceLock::new();
    static TRAILING_BRACKET: OnceLock<Regex> = OnceLock::new();

    let year = YEAR_PREFIX.get_or_init(|| Regex::new(r"^\s*\d{4}\s*[.\-)]?\s*").unwrap());
    let track = TRACK_PREFIX.get_or_init(|| Regex::new(r"^\s*\d{1,3}\s*[.\-)]\s*").unwrap());
    let bracket =
        TRAILING_BRACKET.get_or_init(|| Regex::new(r"\s*[\[\(][^\]\)]*[\]\)]\s*$").unwrap());

    let mut s = raw.trim().to_string();
    // Repeatedly — a component may carry several (`Album Title [Deluxe] (2020)`).
    loop {
        let stripped = bracket.replace(&s, "").into_owned();
        let stripped = stripped.trim_end().to_string();
        if stripped == s {
            break;
        }
        s = stripped;
    }
    // Both prefixes only strip when something non-empty is left behind.
    if let Some(m) = year.find(&s) {
        let rest = &s[m.end()..];
        if !rest.trim().is_empty() {
            s = rest.trim_start().to_string();
        }
    }
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
    // All-digit remainders are noise: bare years, track-listing fragments.
    if s.chars().all(|c| c.is_ascii_digit() || c.is_whitespace()) {
        return None;
    }
    Some(s)
}

/// The file stem, minus a leading track number, unless it's a generic name
/// (`cover`, `booklet-01`, `disc 1`, …) — those carry no signal.
///
/// Audio filenames never reach here: their stems are overwhelmingly track
/// titles, the wrong pool for Artist / Album autocomplete. See
/// `enumerate_filename_inputs` in `fast_pass`.
pub(crate) fn parse_filename_stem(path: &Path) -> Vec<String> {
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

/// Where a `SourcedLine` came from — the clustering pipeline scores members by
/// this. Folder brackets have no variant: they bypass this pipeline entirely,
/// riding `Pool::bracket_catalogs` straight to the catalog output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    Artwork(PathBuf),
    PathComponent,
    FilenameGeneric(PathBuf),
    CueField,
    TextFile(PathBuf),
}

/// A candidate line tagged with its provenance. `text` stays verbatim for
/// display; normalization is internal to clustering.
#[derive(Debug, Clone)]
pub(crate) struct SourcedLine {
    pub source: Source,
    pub text: String,
}

/// Minimum normalized length before a line is eligible for clustering.
/// Shorter strings make Jaro-Winkler similarity unreliable.
const MIN_CLUSTER_LEN: usize = 4;

/// Jaro-Winkler similarity floor for cluster membership.
const JW_THRESHOLD: f64 = 0.85;

/// Upper bound on the free-text pool size.
pub(crate) const FREE_TEXT_CAP: usize = 30;

/// Baseline free-text cutoff: clusters with at least this score survive.
pub(crate) const FREE_TEXT_MIN_SCORE: usize = 2;

/// Fallback free-text cutoff: when no cluster clears the min-score gate (e.g. a
/// single-image candidate), take the top N by score rather than return nothing.
pub(crate) const FREE_TEXT_FALLBACK_TOP: usize = 15;

/// One cluster of sourced lines grouped by normalized-text similarity.
#[derive(Debug, Clone)]
pub(crate) struct Cluster {
    pub normalized_centroid: String,
    pub members: Vec<SourcedLine>,
}

impl Cluster {
    /// Cluster score: the sum of its members' source weights.
    pub(crate) fn score(&self) -> usize {
        self.members.iter().map(|m| source_weight(&m.source)).sum()
    }

    /// The cluster's display form. Preference order:
    ///
    /// 1. Highest source weight (CUE > path component > filename / OCR / txt).
    /// 2. Title-case over ALL-CAPS over all-lowercase.
    /// 3. Longest text — a garbled OCR variant loses characters, so the longer
    ///    of two spellings of one name is usually the real one.
    ///
    /// Members under 4 chars are skipped so truncated OCR fragments can't win.
    /// If every member is below that floor, take the first.
    pub(crate) fn pick_representative(&self) -> String {
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

/// CUE fields and path components are curated by rippers and users, so they
/// carry the strongest per-line signal; artwork OCR is weak per line but earns
/// score by repeating across images.
pub(crate) fn source_weight(source: &Source) -> usize {
    match source {
        Source::CueField => 5,
        Source::PathComponent => 3,
        Source::FilenameGeneric(_) => 1,
        Source::Artwork(_) => 1,
        Source::TextFile(_) => 1,
    }
}

/// Tie-break rank for `pick_representative`: mixed case beats ALL-CAPS beats
/// all-lowercase.
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

/// The clustering key for a line — never displayed. NFD decompose → drop
/// combining marks (so diacritics go) → lowercase → collapse whitespace runs →
/// strip leading/trailing non-alphanumerics.
pub(crate) fn normalize(text: &str) -> String {
    let decomposed: String = text
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect();
    let s = decomposed.to_lowercase();
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
    collapsed
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

/// Append each of `new_lines` into its best-matching cluster, or start a new one.
/// Mutates `clusters` in place so a caller can hold it across calls instead of
/// re-clustering the whole pool on every emission.
pub(crate) fn cluster_lines_incremental(clusters: &mut Vec<Cluster>, new_lines: &[SourcedLine]) {
    for line in new_lines {
        let norm = normalize(&line.text);
        if norm.is_empty() {
            continue;
        }
        if norm.chars().count() < MIN_CLUSTER_LEN {
            // Too short to compare, but still emitted as a singleton — dropping
            // it would lose real 2-3 char album titles.
            clusters.push(Cluster {
                normalized_centroid: norm,
                members: vec![line.clone()],
            });
            continue;
        }

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
pub(crate) fn rank_clusters_in_place(clusters: &mut [Cluster]) {
    clusters.sort_by_key(|c| std::cmp::Reverse(c.score()));
}

/// The surviving clusters as display strings: those scoring at least
/// `FREE_TEXT_MIN_SCORE`, capped at `FREE_TEXT_CAP`. When none clears the gate
/// (common on single-image candidates, where every cluster is score-1), fall back
/// to the top `FREE_TEXT_FALLBACK_TOP` — a useful dropdown beats an empty one.
pub(crate) fn apply_free_text_cutoff(ranked: &[Cluster]) -> Vec<String> {
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
#[path = "candidate_text_tests.rs"]
mod tests;
