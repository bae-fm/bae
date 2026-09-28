//! When an artist credit's name says who the artist is.

/// Whether `name` is the credit a catalog gives a compilation instead of an
/// artist: "Various" or "Various Artists", in any case.
pub(crate) fn is_various_artists(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    lower == "various" || lower == "various artists"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compilation_credit_is_recognized_in_any_case() {
        assert!(is_various_artists("Various Artists"));
        assert!(is_various_artists(" various "));
        assert!(!is_various_artists("Various Artists Band"));
    }
}
