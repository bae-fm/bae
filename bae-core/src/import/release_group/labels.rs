//! What a row says its labels are.

use super::Pressing;

/// Whether two records state the same label: names of one label, and the
/// same number, each compared the way two spellings of one are.
fn same_label(a: &crate::pressing::ReleaseLabel, b: &crate::pressing::ReleaseLabel) -> bool {
    let same_name = match (a.name(), b.name()) {
        (Some(a), Some(b)) => crate::text_match::same_label_name(a, b),
        (a, b) => a.is_none() && b.is_none(),
    };
    same_name
        && a.catalog_number().and_then(crate::text_match::catalog_key)
            == b.catalog_number().and_then(crate::text_match::catalog_key)
}

impl Pressing {
    /// Why a record of the row's document could not be read, where one could
    /// not: the row then states what its search result said.
    pub fn document_failure(&self) -> Option<&crate::signals::LookupFailure> {
        self.releases
            .iter()
            .find_map(|release| release.document_failure.as_ref())
    }

    /// Every label the row's records state, as one list in the order of the
    /// record the row leads with: the lead's labels, then each label another
    /// record states that the lead does not, in that record's order.
    pub fn labels(&self) -> Vec<crate::pressing::ReleaseLabel> {
        let mut labels: Vec<crate::pressing::ReleaseLabel> = Vec::new();
        for label in self.releases.iter().flat_map(|release| &release.labels) {
            if !labels.iter().any(|stated| same_label(stated, label)) {
                labels.push(label.clone());
            }
        }
        labels
    }

    /// The row's labels as it shows them: each name once, each catalog
    /// number once, as [`crate::pressing::label_lines()`] groups them.
    pub fn label_lines(&self) -> Vec<crate::pressing::LabelLine> {
        crate::pressing::label_lines(&self.labels())
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::search::MetadataResult;
    use crate::import::Catalog;
    use crate::pressing::ReleaseLabel;

    fn record(source: Catalog, id: &str, labels: &[(&str, &str)]) -> MetadataResult {
        MetadataResult {
            labels: labels
                .iter()
                .map(|(name, number)| ReleaseLabel::of(Some(name), Some(number)))
                .collect(),
            ..MetadataResult::for_test(source, id, None)
        }
    }

    fn line(name: &str, numbers: &[&str]) -> crate::pressing::LabelLine {
        crate::pressing::LabelLine {
            names: vec![name.to_string()],
            catalog_numbers: numbers.iter().map(|number| number.to_string()).collect(),
        }
    }

    /// Labels that share a name show it once, followed by every number, at
    /// the place the name first appears; storage keeps one entry per label.
    #[test]
    fn a_name_is_shown_once_with_every_number_under_it() {
        let pressing = Pressing {
            releases: vec![record(
                Catalog::Discogs,
                "dg-1",
                &[
                    ("Label One", "AB-100"),
                    ("Label Two", "CD-200"),
                    ("label one", "EF-300"),
                ],
            )],
        };
        assert_eq!(pressing.labels().len(), 3);
        assert_eq!(
            pressing.label_lines(),
            vec![
                line("Label One", &["AB-100", "EF-300"]),
                line("Label Two", &["CD-200"]),
            ]
        );
    }

    /// A row two catalogs name shows one list, in the order of the record it
    /// leads with; what only the other record states follows.
    #[test]
    fn a_merged_row_lists_its_labels_in_its_lead_s_order() {
        let pressing = Pressing {
            releases: vec![
                record(
                    Catalog::MusicBrainz,
                    "mb-1",
                    &[("Label One", "EF-300"), ("Label One", "AB-100")],
                ),
                record(
                    Catalog::Discogs,
                    "dg-1",
                    &[
                        ("Label One", "AB-100"),
                        ("Label Two", "CD-200"),
                        ("Label One", "EF 300"),
                    ],
                ),
            ],
        };
        assert_eq!(
            pressing.label_lines(),
            vec![
                line("Label One", &["EF-300", "AB-100"]),
                line("Label Two", &["CD-200"]),
            ]
        );
    }

    /// Two records that write one label two ways — with and without the
    /// trade word it trails — are paired as one pressing on that label, and
    /// the row lists it once.
    #[test]
    fn a_label_written_two_ways_is_listed_once() {
        let pressing = Pressing {
            releases: vec![
                record(Catalog::MusicBrainz, "mb-1", &[("Harbor Records", "AB-100")]),
                record(Catalog::Discogs, "dg-1", &[("Harbor", "AB 100")]),
            ],
        };
        let paired = crate::import::pressing_evidence::PressingEvidence::between(
            &crate::import::pressing_evidence::ComparedPressing::of(&pressing.releases[0]),
            &crate::import::pressing_evidence::ComparedPressing::of(&pressing.releases[1]),
        );
        assert_eq!(
            paired.label,
            crate::import::pressing_evidence::Comparison::Same
        );
        assert_eq!(pressing.labels().len(), 1);
        assert_eq!(
            pressing.label_lines(),
            vec![line("Harbor Records", &["AB-100"])]
        );
    }
}
