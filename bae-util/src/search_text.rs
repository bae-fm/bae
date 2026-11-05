//! The folding library search compares text by.

/// A text as library search compares it: lowercased by Unicode's rules. The
/// schema's `bae_fold_case` folds each searched title and name this way, and
/// a query is folded the same way before it is matched against them.
pub fn fold_case(text: &str) -> String {
    text.to_lowercase()
}

/// The `GLOB` pattern matching `query` anywhere inside a text folded as the
/// search tables fold theirs: the folded query with `*`, `?` and `[`, which
/// `GLOB` reads as wildcards, each written as the one-character class that
/// matches only itself. The schema's `bae_search_pattern` is this, for the
/// import list's search.
pub fn search_pattern(query: &str) -> String {
    let mut pattern = String::from("*");
    for character in fold_case(query).chars() {
        match character {
            '*' => pattern.push_str("[*]"),
            '?' => pattern.push_str("[?]"),
            '[' => pattern.push_str("[[]"),
            other => pattern.push(other),
        }
    }
    pattern.push('*');
    pattern
}
