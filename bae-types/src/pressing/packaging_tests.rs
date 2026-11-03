use super::*;

#[test]
fn the_table_is_musicbrainzs_own_list() {
    assert_eq!(
        MUSICBRAINZ_RELEASE_PACKAGING.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        crate::pressing::recorded(include_str!(
            "../../test-fixtures/pressing-vocabulary/musicbrainz-release-packaging.txt"
        ))
    );
    let named: Vec<_> =
        MUSICBRAINZ_RELEASE_PACKAGING.iter().map(|(_, packaging)| *packaging).collect();
    assert_eq!(named, Packaging::ALL, "one packaging per MusicBrainz name");
}

#[test]
fn none_is_a_packaging_rather_than_an_unstated_one() {
    assert_eq!(Packaging::musicbrainz("None"), Some(Packaging::Unpackaged));
    assert_eq!(Packaging::musicbrainz("cardboard/paper sleeve"), Some(Packaging::CardboardSleeve));
    assert_eq!(Packaging::musicbrainz("Digisleeve"), None);
}
