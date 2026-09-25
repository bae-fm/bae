use super::*;

#[test]
fn normalize_strips_diacritics() {
    assert_eq!(normalize("Café"), "cafe");
    assert_eq!(normalize("Fjörn"), "fjorn");
}

#[test]
fn normalize_lowercases_and_collapses_whitespace() {
    assert_eq!(normalize("  Album   Title  "), "album title");
    assert_eq!(normalize("Album\tTitle"), "album title");
}

#[test]
fn normalize_strips_leading_trailing_nonalnum() {
    assert_eq!(normalize("\"Album Title\""), "album title");
    assert_eq!(normalize("!!!Album!!!"), "album");
}

#[test]
fn normalize_folds_an_artist_name_however_it_is_cased_or_accented() {
    assert_eq!(normalize("Artist Name"), "artist name");
    assert_eq!(normalize("artist name"), "artist name");
    assert_eq!(normalize("ÄRTIST  Name"), "artist name");
    assert_eq!(normalize("Ärtist Name"), "artist name");
}
