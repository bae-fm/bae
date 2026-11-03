use super::*;

#[test]
fn squash_keeps_only_letters_and_digits() {
    assert_eq!(squash("WPCR-80001"), "wpcr80001");
    assert_eq!(squash("WPCR 80001"), "wpcr80001");
    assert_eq!(squash("Ärtist Näme"), "artistname");
}

#[test]
fn words_are_the_squashed_runs_of_letters_and_digits() {
    assert_eq!(words("16033-2"), vec!["16033", "2"]);
    assert_eq!(words("  Ärtist  Näme! "), vec!["artist", "name"]);
    assert_eq!(words("Ärtist Näme").concat(), squash("Ärtist Näme"));
    assert!(words("··· —").is_empty());
}

#[test]
fn written_words_keep_case_and_drop_dots_between_letters() {
    assert_eq!(written_words("D.F.C. Records"), vec!["DFC", "Records"]);
    assert_eq!(written_words("Made in E.U."), vec!["Made", "in", "EU"]);
}

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
