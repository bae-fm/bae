use super::*;

#[test]
fn releases_of_one_collapses_to_release() {
    assert_eq!(ContextSource::releases(vec!["only".into()]), ContextSource::Release("only".into()));
}

#[test]
fn releases_of_many_is_releases() {
    assert_eq!(
        ContextSource::releases(vec!["a".into(), "b".into(), "c".into()]),
        ContextSource::Releases
    );
}

/// Every source kind survives the columns' encode and decode, and none is
/// mistaken for another.
#[test]
fn source_columns_round_trip() {
    for source in [
        ContextSource::Release("550e8400-e29b-41d4-a716-446655440000".into()),
        ContextSource::Releases,
        ContextSource::Library,
    ] {
        let (kind, release_id) = source.to_columns();
        assert_eq!(ContextSource::from_columns(kind, release_id.map(str::to_string)), Ok(source));
    }
}

#[test]
fn a_pair_bae_never_writes_is_refused() {
    assert!(ContextSource::from_columns("release", None).is_err());
    assert!(ContextSource::from_columns("library", Some("id".into())).is_err());
    assert!(ContextSource::from_columns("folder", None).is_err());
}
