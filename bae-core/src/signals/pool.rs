//! The text pool behind the classifier: it gathers lines and folder-bracket
//! catalog numbers, dedups them, and re-classifies incrementally. The lines
//! also go out unchanged as the candidate's own text.

use super::candidate_text::{
    self, apply_free_text_cutoff, catalog_numbers, cluster_lines_incremental,
    rank_clusters_in_place, strip_path_component, Cluster, Source, SourcedLine,
};
use crate::signals::{TextLine, TextOrigin};
use std::collections::HashSet;

/// `bracket_catalogs` go straight to the catalog output, skipping the catalog
/// regex, which is too strict for formats like `Z1 12345`. `clusters` and
/// `clustered_through` let `classify` cluster only lines added since last time.
#[derive(Default)]
pub(super) struct Pool {
    pub(super) lines: Vec<SourcedLine>,
    pub(super) bracket_catalogs: Vec<String>,
    seen: HashSet<(Source, String)>,
    clusters: Vec<Cluster>,
    clustered_through: usize,
}

pub(super) struct Classification {
    pub(super) catalogs: Vec<String>,
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

    /// Classify the pool: catalog numbers over every line, free text over only
    /// the lines added since the last call.
    pub(super) fn classify(&mut self) -> Classification {
        let mut catalogs = catalog_numbers(&self.lines);
        let mut seen_catalog: HashSet<String> = catalogs.iter().cloned().collect();
        for extra in &self.bracket_catalogs {
            if seen_catalog.insert(extra.clone()) {
                catalogs.push(extra.clone());
            }
        }

        // A rejected line never clusters; a path component clusters by what is
        // left once its year prefix and bracketed tail are off.
        let new_slice = &self.lines[self.clustered_through..];
        let filtered: Vec<SourcedLine> = new_slice
            .iter()
            .filter_map(clustered_form)
            .filter(|l| !candidate_text::should_reject_line(&l.text))
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

    /// Every line the pass read, as read, in gathering order.
    pub(super) fn text_lines(&self) -> Vec<TextLine> {
        self.lines
            .iter()
            .map(|line| TextLine {
                text: line.text.clone(),
                origin: TextOrigin::of_source(&line.source),
            })
            .collect()
    }
}

/// The line as clustering sees it: a path component reduced to its name,
/// `None` when nothing is left; anything else unchanged.
fn clustered_form(line: &SourcedLine) -> Option<SourcedLine> {
    match line.source {
        Source::PathComponent => strip_path_component(&line.text).map(|text| SourcedLine {
            text,
            ..line.clone()
        }),
        _ => Some(line.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // MARK: - Pool::classify end-to-end (one-shot on a fresh pool)

    fn cue_line(text: &str) -> SourcedLine {
        SourcedLine::new(
            Source::CueField {
                file_id: "Album.cue".to_string(),
            },
            text.to_string(),
        )
    }

    fn path_line(text: &str) -> SourcedLine {
        SourcedLine::new(Source::PathComponent, text.to_string())
    }

    fn artwork_line(path: &str, text: &str) -> SourcedLine {
        SourcedLine::new(
            Source::Artwork {
                path: PathBuf::from(path),
            },
            text.to_string(),
        )
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
            classification.catalogs,
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
