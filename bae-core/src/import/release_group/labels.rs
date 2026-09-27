//! What a row says its labels are.

use super::Pressing;

/// One label as a row shows it: its name once, then every catalog number the
/// row has under that name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelLine {
    pub name: Option<String>,
    pub catalog_numbers: Vec<String>,
}

/// Whether two records state the same label: the same name and number, each
/// compared the way two spellings of one are.
fn same_label(a: &crate::pressing::ReleaseLabel, b: &crate::pressing::ReleaseLabel) -> bool {
    a.name().map(crate::util::text::normalize) == b.name().map(crate::util::text::normalize)
        && a.catalog_number().and_then(crate::util::text::catalog_key)
            == b.catalog_number().and_then(crate::util::text::catalog_key)
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

    /// The row's labels as it shows them: a name once, followed by every
    /// catalog number the row has under it, at the place the name first
    /// appears. A number stated with no name is shown on its own.
    pub fn label_lines(&self) -> Vec<LabelLine> {
        let mut lines: Vec<LabelLine> = Vec::new();
        for label in self.labels() {
            let number = label.catalog_number().map(str::to_string);
            let named = label.name().and_then(|name| {
                lines.iter_mut().find(|line| {
                    line.name.as_deref().is_some_and(|shown| {
                        crate::util::text::normalize(shown) == crate::util::text::normalize(name)
                    })
                })
            });
            match named {
                Some(line) => line.catalog_numbers.extend(number),
                None => lines.push(LabelLine {
                    name: label.name().map(str::to_string),
                    catalog_numbers: number.into_iter().collect(),
                }),
            }
        }
        lines
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

    fn line(name: &str, numbers: &[&str]) -> LabelLine {
        LabelLine {
            name: Some(name.to_string()),
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
}
