//! The collation a list's descending sort columns declare.

/// Byte order run backwards, so a column in this collation sorts ascending
/// where its text would sort descending.
pub fn compare(left: &str, right: &str) -> std::cmp::Ordering {
    right.cmp(left)
}
