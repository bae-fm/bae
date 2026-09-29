//! When an artist credit's name says who the artist is.

/// An artist's name as a library artist is found by it: [`super::normalize`]d,
/// so a credit meets the artist it names however either is cased, accented
/// or spaced. A library artist stores it as its `name_key`.
pub(crate) fn artist_name_key(name: &str) -> String {
    super::normalize(name)
}

/// Whether two artist names name one artist, as far as a name says: the same
/// [`artist_name_key`], and not an empty one — a name with no letters or
/// digits names nobody.
pub(crate) fn same_artist_name(a: &str, b: &str) -> bool {
    let key = artist_name_key(a);
    !key.is_empty() && key == artist_name_key(b)
}

/// Whether `name` is the credit a catalog gives a compilation instead of an
/// artist: "Various" or "Various Artists", in any case.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn is_various_artists(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    lower == "various" || lower == "various artists"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_artist_however_the_name_is_cased_accented_or_spaced() {
        assert!(same_artist_name("Ärtist Name", "artist  name"));
        assert!(same_artist_name("ARTIST NAME.", "Artist Name"));
        assert!(!same_artist_name("Artist Name", "Other Name"));
        assert!(!same_artist_name("", ""));
        assert!(!same_artist_name("···", "—"));
    }

    #[test]
    fn the_compilation_credit_is_recognized_in_any_case() {
        assert!(is_various_artists("Various Artists"));
        assert!(is_various_artists(" various "));
        assert!(!is_various_artists("Various Artists Band"));
    }
}
