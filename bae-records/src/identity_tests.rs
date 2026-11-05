use super::*;

#[test]
fn a_fact_has_one_id_and_different_facts_do_not_share_one() {
    assert_eq!(album_artist_id("a", 0), album_artist_id("a", 0));
    assert_ne!(album_artist_id("a", 0), album_artist_id("a", 1));
    assert_ne!(album_artist_id("a", 0), track_artist_id("a", 0));
    assert_ne!(
        artist_id(Some("x"), None),
        artist_id(None, Some("x")),
        "a MusicBrainz and a Discogs id with one spelling are different artists"
    );
    assert_eq!(artist_id(Some("x"), Some("y")), artist_id(Some("x"), None));
    assert_eq!(artist_id(None, None), None);
    assert_ne!(label_id(Some("x"), None), artist_id(Some("x"), None));
    assert_eq!(label_id(Some("x"), Some("y")), label_id(Some("x"), None));
    assert_eq!(label_id(None, None), None);
}
