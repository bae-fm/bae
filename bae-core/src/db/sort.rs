//! In-memory equivalent of the SQL ORDER BY built in [`super::client`].
//!
//! Live album lists go through the database, which applies its own
//! comparator via the SQL `ORDER BY` clause that
//! `client::build_order_by` produces. In-memory consumers — SwiftUI
//! previews, tests that build album rows by hand, anything operating
//! on a `Vec<AlbumSummary>` without a database — need the same
//! ordering, otherwise preview/test output diverges from what the
//! live grid shows.
//!
//! [`sort_albums`] is that in-memory comparator. The semantics here
//! must stay aligned with `build_order_by` in `client.rs`; tests in
//! this module assert the rules each side relies on.
//!
//! ## Rules
//!
//! - `Title` / `Artist`: ASCII case-insensitive (mirrors SQLite
//!   `COLLATE NOCASE`).
//! - `Year`: known years compared numerically; unknown (`None`) years
//!   sort *after* every known year on ascending and *before* every
//!   known year on descending. The DB does this with
//!   `CASE WHEN a.year IS NULL THEN 1 ELSE 0 END <dir>, a.year <dir>`.
//! - `DateAdded`: the in-memory `AlbumSummary` does not carry
//!   `created_at`, so this field leaves order untouched. Callers that
//!   need date-added order go through the database.
//! - Multiple criteria chain as primary/secondary: ties on the first
//!   criterion break on the next, matching the comma-separated SQL
//!   clause.
//! - Empty criteria slice is a no-op — leaves input order intact.

use std::cmp::Ordering;

use crate::album_detail::AlbumSummary;
use crate::db::models::{AlbumSortCriterion, AlbumSortField, SortDirection};

pub fn sort_albums(albums: &mut [AlbumSummary], criteria: &[AlbumSortCriterion]) {
    if criteria.is_empty() {
        return;
    }
    albums.sort_by(|a, b| compare(a, b, criteria));
}

fn compare(a: &AlbumSummary, b: &AlbumSummary, criteria: &[AlbumSortCriterion]) -> Ordering {
    for c in criteria {
        let ord = compare_field_asc(a, b, c.field);
        let ord = match c.direction {
            SortDirection::Ascending => ord,
            SortDirection::Descending => ord.reverse(),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

fn compare_field_asc(a: &AlbumSummary, b: &AlbumSummary, field: AlbumSortField) -> Ordering {
    match field {
        AlbumSortField::Title => compare_nocase(&a.title, &b.title),
        AlbumSortField::Artist => compare_nocase(&a.artist_names, &b.artist_names),
        AlbumSortField::Year => compare_year_asc(a.year, b.year),
        AlbumSortField::DateAdded => Ordering::Equal,
    }
}

fn compare_nocase(a: &str, b: &str) -> Ordering {
    a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase())
}

fn compare_year_asc(a: Option<i32>, b: Option<i32>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn album(id: &str, title: &str, artist_names: &str, year: Option<i32>) -> AlbumSummary {
        AlbumSummary {
            id: id.to_string(),
            title: title.to_string(),
            year,
            is_compilation: false,
            artist_names: artist_names.to_string(),
            release_ids: vec![format!("{id}-r")],
            primary_release_id: format!("{id}-r"),
            cover: None,
        }
    }

    fn ids(albums: &[AlbumSummary]) -> Vec<&str> {
        albums.iter().map(|a| a.id.as_str()).collect()
    }

    fn criterion(field: AlbumSortField, direction: SortDirection) -> AlbumSortCriterion {
        AlbumSortCriterion { field, direction }
    }

    #[test]
    fn empty_criteria_is_noop() {
        let mut albums = vec![
            album("a", "Zebra", "Z", Some(2020)),
            album("b", "Apple", "A", Some(1970)),
        ];

        sort_albums(&mut albums, &[]);

        assert_eq!(ids(&albums), vec!["a", "b"]);
    }

    #[test]
    fn title_ascending_is_case_insensitive() {
        let mut albums = vec![
            album("a", "banana", "X", None),
            album("b", "Apple", "X", None),
            album("c", "cherry", "X", None),
        ];

        sort_albums(
            &mut albums,
            &[criterion(AlbumSortField::Title, SortDirection::Ascending)],
        );

        assert_eq!(ids(&albums), vec!["b", "a", "c"]);
    }

    #[test]
    fn artist_descending_is_case_insensitive() {
        let mut albums = vec![
            album("a", "X", "alpha", None),
            album("b", "X", "Bravo", None),
            album("c", "X", "charlie", None),
        ];

        sort_albums(
            &mut albums,
            &[criterion(AlbumSortField::Artist, SortDirection::Descending)],
        );

        assert_eq!(ids(&albums), vec!["c", "b", "a"]);
    }

    #[test]
    fn year_ascending_puts_unknowns_last() {
        let mut albums = vec![
            album("a", "X", "X", None),
            album("b", "X", "X", Some(2020)),
            album("c", "X", "X", None),
            album("d", "X", "X", Some(1970)),
        ];

        sort_albums(
            &mut albums,
            &[criterion(AlbumSortField::Year, SortDirection::Ascending)],
        );

        assert_eq!(albums[0].year, Some(1970));
        assert_eq!(albums[1].year, Some(2020));
        assert_eq!(albums[2].year, None);
        assert_eq!(albums[3].year, None);
    }

    #[test]
    fn year_descending_puts_unknowns_first() {
        let mut albums = vec![
            album("a", "X", "X", Some(1970)),
            album("b", "X", "X", None),
            album("c", "X", "X", Some(2020)),
            album("d", "X", "X", None),
        ];

        sort_albums(
            &mut albums,
            &[criterion(AlbumSortField::Year, SortDirection::Descending)],
        );

        assert_eq!(albums[0].year, None);
        assert_eq!(albums[1].year, None);
        assert_eq!(albums[2].year, Some(2020));
        assert_eq!(albums[3].year, Some(1970));
    }

    #[test]
    fn secondary_criterion_breaks_ties() {
        let mut albums = vec![
            album("a", "Beta", "Solo", Some(2000)),
            album("b", "Alpha", "Solo", Some(2000)),
            album("c", "Gamma", "Solo", Some(1990)),
        ];

        sort_albums(
            &mut albums,
            &[
                criterion(AlbumSortField::Year, SortDirection::Ascending),
                criterion(AlbumSortField::Title, SortDirection::Ascending),
            ],
        );

        assert_eq!(ids(&albums), vec!["c", "b", "a"]);
    }

    #[test]
    fn date_added_is_passthrough() {
        let mut albums = vec![
            album("a", "Z", "Z", Some(2020)),
            album("b", "A", "A", Some(1970)),
            album("c", "M", "M", None),
        ];

        sort_albums(
            &mut albums,
            &[criterion(
                AlbumSortField::DateAdded,
                SortDirection::Ascending,
            )],
        );

        assert_eq!(ids(&albums), vec!["a", "b", "c"]);
    }
}
