//! A list of labels as a surface shows it: each name once, each catalog
//! number once.

use super::ReleaseLabel;
use crate::text_match::{catalog_key, same_label_name};

/// One line of a label list: the names the numbers are released on, then the
/// catalog numbers they share. A name stated with several numbers shows once
/// with all of them; names stated with the same numbers show together, the
/// numbers once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelLine {
    pub names: Vec<String>,
    pub catalog_numbers: Vec<String>,
}

/// `labels` as lines, in the order each first appears: first every number
/// under the name it is stated with — the first name of its label, as
/// `same_label_name` tells labels apart — then every name whose numbers are
/// the same as an earlier name's beside that name. A number stated with no
/// name is its own line.
pub fn label_lines(labels: &[ReleaseLabel]) -> Vec<LabelLine> {
    let mut by_name: Vec<LabelLine> = Vec::new();
    for label in labels {
        let number = label.catalog_number().map(str::to_string);
        let named = label.name().and_then(|name| {
            by_name
                .iter_mut()
                .find(|line| line.names.first().is_some_and(|shown| same_label_name(shown, name)))
        });
        match named {
            Some(line) => line.catalog_numbers.extend(number),
            None => by_name.push(LabelLine {
                names: label.name().map(str::to_string).into_iter().collect(),
                catalog_numbers: number.into_iter().collect(),
            }),
        }
    }
    let mut lines: Vec<LabelLine> = Vec::new();
    for line in by_name {
        let shared = (!line.names.is_empty() && !line.catalog_numbers.is_empty())
            .then(|| {
                lines.iter_mut().find(|shown| !shown.names.is_empty() && same_numbers(shown, &line))
            })
            .flatten();
        match shared {
            Some(shown) => shown.names.extend(line.names),
            None => lines.push(line),
        }
    }
    lines
}

/// Whether two lines state the same catalog numbers, each compared the way
/// two spellings of one are.
fn same_numbers(a: &LabelLine, b: &LabelLine) -> bool {
    let keys = |line: &LabelLine| {
        let mut keys: Vec<String> =
            line.catalog_numbers.iter().filter_map(|number| catalog_key(number)).collect();
        keys.sort();
        keys
    };
    keys(a) == keys(b)
}

#[cfg(test)]
#[path = "label_lines_tests.rs"]
mod tests;
