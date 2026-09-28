//! A list of labels as a surface shows it: each name once, each catalog
//! number once.

use super::ReleaseLabel;
use crate::text_match::{catalog_key, normalize};

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
/// under the name it is stated with, then every name whose numbers are the
/// same as an earlier name's beside that name. A number stated with no name
/// is its own line.
pub fn label_lines(labels: &[ReleaseLabel]) -> Vec<LabelLine> {
    let mut by_name: Vec<LabelLine> = Vec::new();
    for label in labels {
        let number = label.catalog_number().map(str::to_string);
        let named = label.name().and_then(|name| {
            by_name.iter_mut().find(|line| {
                line.names
                    .first()
                    .is_some_and(|shown| normalize(shown) == normalize(name))
            })
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
                lines
                    .iter_mut()
                    .find(|shown| !shown.names.is_empty() && same_numbers(shown, &line))
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
        let mut keys: Vec<String> = line
            .catalog_numbers
            .iter()
            .filter_map(|number| catalog_key(number))
            .collect();
        keys.sort();
        keys
    };
    keys(a) == keys(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(name: Option<&str>, number: Option<&str>) -> ReleaseLabel {
        ReleaseLabel::of(name, number)
    }

    fn line(names: &[&str], numbers: &[&str]) -> LabelLine {
        LabelLine {
            names: names.iter().map(|name| name.to_string()).collect(),
            catalog_numbers: numbers.iter().map(|number| number.to_string()).collect(),
        }
    }

    /// A name stated with several numbers shows once, with every number, at
    /// the place the name first appears.
    #[test]
    fn a_name_is_shown_once_with_every_number_under_it() {
        let labels = [
            label(Some("Label One"), Some("AB-100")),
            label(Some("Label Two"), Some("CD-200")),
            label(Some("label one"), Some("EF-300")),
        ];
        assert_eq!(
            label_lines(&labels),
            vec![
                line(&["Label One"], &["AB-100", "EF-300"]),
                line(&["Label Two"], &["CD-200"]),
            ]
        );
    }

    /// Names stated with the same number show together, the number once, at
    /// the place the first of them appears.
    #[test]
    fn names_sharing_a_number_show_it_once() {
        let labels = [
            label(Some("Label One"), Some("AB-100")),
            label(Some("Label Two"), Some("AB 100")),
            label(Some("Label Three"), Some("CD-200")),
            label(Some("Label Four"), Some("AB-100")),
        ];
        assert_eq!(
            label_lines(&labels),
            vec![
                line(&["Label One", "Label Two", "Label Four"], &["AB-100"]),
                line(&["Label Three"], &["CD-200"]),
            ]
        );
    }

    /// Names only group when every number is the same; a number with no name
    /// and a name with no number each stand alone.
    #[test]
    fn only_the_same_numbers_group_names() {
        let labels = [
            label(Some("Label One"), Some("AB-100")),
            label(Some("Label One"), Some("CD-200")),
            label(Some("Label Two"), Some("AB-100")),
            label(None, Some("EF-300")),
            label(Some("Label Three"), None),
            label(Some("Label Four"), None),
        ];
        assert_eq!(
            label_lines(&labels),
            vec![
                line(&["Label One"], &["AB-100", "CD-200"]),
                line(&["Label Two"], &["AB-100"]),
                line(&[], &["EF-300"]),
                line(&["Label Three"], &[]),
                line(&["Label Four"], &[]),
            ]
        );
    }
}
