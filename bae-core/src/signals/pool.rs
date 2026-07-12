//! The accumulating text pool behind the classifier: it gathers source-tagged
//! lines and folder-bracket catalogs, dedups them, and re-classifies
//! incrementally as the extraction pass adds more lines.

use super::candidate_text::{
    self, apply_free_text_cutoff, catalog_numbers_sourced, cluster_lines_incremental,
    rank_clusters_in_place, Cluster, Source, SourcedLine,
};
use crate::signals::{SignalOrigin, SourcedValue};
use std::collections::HashSet;

/// `lines` is the source-tagged text that gets filtered / clustered / ranked.
/// `bracket_catalogs` are folder-bracket extractions routed straight to the
/// catalog output, bypassing the free-text catalog regex (too strict for
/// real-world formats like `Z1 12345`).
///
/// Two caches ride alongside: `seen` keeps `push` dedup O(1) as the pool grows,
/// and `clusters` + `clustered_through` let `classify` cluster only the lines
/// added since the last call rather than the whole pool each emission.
#[derive(Default)]
pub(super) struct Pool {
    pub(super) lines: Vec<SourcedLine>,
    pub(super) bracket_catalogs: Vec<String>,
    seen: HashSet<(Source, String)>,
    clusters: Vec<Cluster>,
    clustered_through: usize,
}

pub(super) struct Classification {
    pub(super) catalogs: Vec<SourcedValue>,
    pub(super) free_text: Vec<String>,
}

impl Pool {
    pub(super) fn push(&mut self, line: SourcedLine) {
        if line.text.is_empty() {
            return;
        }
        // Dedupe on (source, text); `lines` stays in insertion order, which is
        // the order clustering walks.
        let key = (line.source.clone(), line.text.clone());
        if !self.seen.insert(key) {
            return;
        }
        self.lines.push(line);
    }

    pub(super) fn push_bracket(&mut self, s: String) {
        if !s.is_empty() && !self.bracket_catalogs.contains(&s) {
            self.bracket_catalogs.push(s);
        }
    }

    /// Classify the pool's current contents. Catalog extraction re-runs the regex
    /// over the whole pool (cheap); free-text clustering is incremental — only
    /// lines added since the last call are matched against the existing clusters.
    pub(super) fn classify(&mut self) -> Classification {
        let mut catalogs = catalog_numbers_sourced(&self.lines);
        let mut seen_catalog: HashSet<String> = catalogs.iter().map(|c| c.value.clone()).collect();
        for extra in &self.bracket_catalogs {
            if seen_catalog.insert(extra.clone()) {
                catalogs.push(SourcedValue::new(extra.clone(), SignalOrigin::FolderName));
            }
        }

        // A line `should_reject_line` drops never feeds a cluster at all.
        let new_slice = &self.lines[self.clustered_through..];
        let filtered: Vec<SourcedLine> = new_slice
            .iter()
            .filter(|l| !candidate_text::should_reject_line(&l.text))
            .cloned()
            .collect();
        cluster_lines_incremental(&mut self.clusters, &filtered);
        self.clustered_through = self.lines.len();

        // Rank on a copy: the pool's own clusters must stay in insertion order,
        // which is what makes the next `classify` incremental.
        let mut ranked = self.clusters.clone();
        rank_clusters_in_place(&mut ranked);
        let free_text = apply_free_text_cutoff(&ranked);

        Classification {
            catalogs,
            free_text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    /// Push lines + brackets into a fresh pool and classify once, catalogs
    /// projected back to bare strings.
    fn classify_pool(lines: Vec<SourcedLine>, brackets: &[&str]) -> (Vec<String>, Vec<String>) {
        let mut pool = Pool::default();
        for line in lines {
            pool.push(line);
        }
        for b in brackets {
            pool.push_bracket((*b).to_string());
        }
        let classification = pool.classify();
        (
            classification
                .catalogs
                .into_iter()
                .map(|c| c.value)
                .collect(),
            classification.free_text,
        )
    }

    #[test]
    fn classify_promotes_high_score_clusters() {
        // Three sources agree on "Artist Alpha"; the reject rules drop the
        // one-off OCR credit line entirely.
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
        // One artwork image, so every cluster is a score-1 singleton. The pool
        // must fall back to the top N rather than come back empty.
        let lines = vec![
            artwork_line("/a.jpg", "Artist Alpha"),
            artwork_line("/a.jpg", "Album Title B"),
            artwork_line("/a.jpg", "Label Name"),
        ];
        let (_, free_text) = classify_pool(lines, &[]);
        assert!(!free_text.is_empty(), "expected fallback top-N pool");
    }
}
