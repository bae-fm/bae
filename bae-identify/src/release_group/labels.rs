//! What a row says its labels are.

use super::Pressing;

/// Whether two records state the same label: names of one label, and the
/// same number, each compared the way two spellings of one are.
fn same_label(
    a: &bae_types::pressing::ReleaseLabel,
    b: &bae_types::pressing::ReleaseLabel,
) -> bool {
    let same_name = match (a.name(), b.name()) {
        (Some(a), Some(b)) => bae_types::text_match::same_label_name(a, b),
        (a, b) => a.is_none() && b.is_none(),
    };
    same_name
        && a.catalog_number().and_then(bae_types::text_match::catalog_key)
            == b.catalog_number().and_then(bae_types::text_match::catalog_key)
}

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
        let mut labels: Vec<bae_types::pressing::ReleaseLabel> = Vec::new();
        for label in self.releases.iter().flat_map(|release| &release.labels) {
            match labels.iter_mut().find(|stated| same_label(stated, label)) {
                Some(stated) => stated.take_missing_ids(label),
                None => labels.push(label.clone()),
            }
        }
        labels
    }

    /// The row's labels as it shows them: each name once, each catalog
    /// number once, as [`bae_types::pressing::label_lines()`] groups them.
    pub fn label_lines(&self) -> Vec<bae_types::pressing::LabelLine> {
        bae_types::pressing::label_lines(&self.labels())
    }
}

#[cfg(test)]
#[path = "labels_tests.rs"]
mod tests;
