//! Compact pressing-metadata line for a release.
//!
//! Pure projection: takes whichever of `year`, `format`, `label`,
//! `catalog_number`, `country` are set on the release, in that order, and
//! joins them with ` · ` (U+00B7 with surrounding spaces). Returns an
//! empty string when none are set. These are the fields that imply which
//! pressing a release is.

const SEPARATOR: &str = " \u{00B7} ";

/// Render the compact pressing-metadata line from a release's columns, in
/// the fixed order year → format → label → catalog number → country.
pub fn release_metadata_compact(
    year: Option<i32>,
    format: Option<&str>,
    label: Option<&str>,
    catalog_number: Option<&str>,
    country: Option<&str>,
) -> String {
    let year = year.map(|y| y.to_string());
    let parts = [year.as_deref(), format, label, catalog_number, country];
    parts
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(SEPARATOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_five_set_concatenates_in_order() {
        assert_eq!(
            release_metadata_compact(
                Some(1999),
                Some("CD"),
                Some("Label Name"),
                Some("CAT-001"),
                Some("US"),
            ),
            "1999 \u{00B7} CD \u{00B7} Label Name \u{00B7} CAT-001 \u{00B7} US"
        );
    }

    #[test]
    fn year_and_format_only_skips_unset() {
        assert_eq!(
            release_metadata_compact(Some(1999), Some("Vinyl"), None, None, None),
            "1999 \u{00B7} Vinyl"
        );
    }

    #[test]
    fn all_unset_returns_empty() {
        assert_eq!(release_metadata_compact(None, None, None, None, None), "");
    }

    #[test]
    fn middle_gap_collapses() {
        // Order is fixed; only-set fields participate.
        assert_eq!(
            release_metadata_compact(Some(1999), None, Some("Label Name"), None, Some("US")),
            "1999 \u{00B7} Label Name \u{00B7} US"
        );
    }

    #[test]
    fn order_is_year_format_label_catno_country() {
        // Provide values that sort lexicographically reverse so any
        // accidental sort would surface.
        let line = release_metadata_compact(
            Some(2020),
            Some("Vinyl"),
            Some("Label"),
            Some("CAT-99"),
            Some("AAA"),
        );
        let positions: Vec<usize> = ["2020", "Vinyl", "Label", "CAT-99", "AAA"]
            .iter()
            .map(|s| line.find(s).expect("part present"))
            .collect();
        assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "parts out of order: {line:?}"
        );
    }
}
