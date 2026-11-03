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
