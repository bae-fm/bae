//! The text pool behind the classifier: it gathers lines and folder-bracket
//! catalog numbers, dedups them, and re-classifies incrementally. The lines
//! also go out unchanged as the candidate's own text.

use super::candidate_text::{
    self, apply_free_text_cutoff, catalog_numbers, cluster_lines_incremental,
    rank_clusters_in_place, strip_path_component, Cluster, Source, SourcedLine,
};
use bae_identify::signals::TextLine;
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

        Classification { catalogs, free_text }
    }

    /// Every line the pass read, as read, in gathering order.
    pub(super) fn text_lines(&self) -> Vec<TextLine> {
        self.lines
            .iter()
            .map(|line| TextLine { text: line.text.clone(), origin: line.source.text_origin() })
            .collect()
    }
}

/// The line as clustering sees it: a path component reduced to its name,
/// `None` when nothing is left; anything else unchanged.
fn clustered_form(line: &SourcedLine) -> Option<SourcedLine> {
    match line.source {
        Source::PathComponent => {
            strip_path_component(&line.text).map(|text| SourcedLine { text, ..line.clone() })
        }
        _ => Some(line.clone()),
    }
}

#[cfg(test)]
#[path = "pool_tests.rs"]
mod tests;
