use super::*;

#[test]
fn the_table_is_musicbrainzs_own_list() {
    assert_eq!(
        MUSICBRAINZ_RELEASE_STATUSES.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        crate::pressing::recorded(include_str!(
            "../../test-fixtures/pressing-vocabulary/musicbrainz-release-statuses.txt"
        ))
    );
    let mut statuses: Vec<_> =
        MUSICBRAINZ_RELEASE_STATUSES.iter().map(|(_, status)| *status).collect();
    statuses.sort_by_key(|status| status.key());
    let mut all = ReleaseStatus::ALL.to_vec();
    all.sort_by_key(|status| status.key());
    assert_eq!(statuses, all, "every status is one MusicBrainz names");
}

#[test]
fn a_name_is_read_whole_and_in_any_case() {
    assert_eq!(ReleaseStatus::musicbrainz("Pseudo-Release"), Some(ReleaseStatus::PseudoRelease));
    assert_eq!(ReleaseStatus::musicbrainz("promotion"), Some(ReleaseStatus::Promotion));
    assert_eq!(ReleaseStatus::musicbrainz("Promo"), None);
}
