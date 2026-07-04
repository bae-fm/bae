//! Diagnostic-corpus dump of a completed scan.
//!
//! Writes the raw sources, the cluster pipeline intermediate, and the final
//! classified output to `~/.bae/candidate-text-scans/<key>.json` for debugging
//! and regression-corpus building. The extraction service spawns [`dump_scan`]
//! after its final emit so a slow filesystem write never gates emission.

use super::pool::Pool;
use super::SourcedValue;
use crate::identify::candidate_text::{self, Source, SourcedLine};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Dump the completed scan to `~/.bae/candidate-text-scans/<key>.json` for
/// debugging and regression-corpus building. Records the raw sources, the
/// cluster pipeline intermediate, and the final classified output.
///
/// Returns `Err` if the dump directory can't be created or the file can't
/// be written. Callers log-and-swallow; dumps must never block emission.
pub(super) fn dump_scan(
    key: &str,
    folder: &Path,
    pool: &Pool,
    catalogs: &[SourcedValue],
    free_text: &[String],
    now: chrono::DateTime<chrono::Utc>,
) -> std::io::Result<()> {
    let Some(home) = dirs::home_dir() else {
        warn!("signals: no home dir, skipping dump");
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
    let suffix = hex::encode(&digest[..4]);
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

    let mut sources_artwork: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    let mut path_components: Vec<String> = Vec::new();
    let mut filenames: Vec<serde_json::Value> = Vec::new();
    let mut cue_fields: Vec<String> = Vec::new();
    let mut text_files: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
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

#[cfg(test)]
mod tests {
    use super::*;

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
