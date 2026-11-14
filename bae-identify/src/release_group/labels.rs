//! What a row says its labels are.

use super::Pressing;
use bae_catalogs::search::MetadataResult;

impl Pressing {
    /// Why a record of the row's document could not be read, where one could
    /// not: the row then states what its search result said.
    pub fn document_failure(&self) -> Option<&bae_catalogs::LookupFailure> {
        self.releases.iter().find_map(|release| release.document_failure.as_ref())
    }

    /// Every label the row's records state, as one list in the order of the
    /// record the row leads with: the lead's labels, then each label another
    /// record states that the lead does not, in that record's order. A label
    /// two records state carries the ids each knows it by.
    pub fn labels(&self) -> Vec<bae_types::pressing::ReleaseLabel> {
        labels_of(&self.releases)
    }

    /// The row's labels as it shows them: each name once, each catalog
    /// number once, as [`bae_types::pressing::label_lines()`] groups them.
    pub fn label_lines(&self) -> Vec<bae_types::pressing::LabelLine> {
        bae_types::pressing::label_lines(&self.labels())
    }
}

/// Every label the records of one row state, as [`Pressing::labels`] lists
/// them.
pub(super) fn labels_of(releases: &[MetadataResult]) -> Vec<bae_types::pressing::ReleaseLabel> {
    let mut labels: Vec<bae_types::pressing::ReleaseLabel> = Vec::new();
    for label in releases.iter().flat_map(|release| &release.labels) {
        match labels.iter_mut().find(|stated| stated.same_label(label)) {
            Some(stated) => stated.take_missing_ids(label),
            None => labels.push(label.clone()),
        }
    }
    labels
}

#[cfg(test)]
#[path = "labels_tests.rs"]
mod tests;
