use super::*;
use crate::pressing::recorded;

#[test]
fn the_table_is_discogss_own_list() {
    assert_eq!(
        DISCOGS_FORMAT_DESCRIPTIONS.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        recorded(include_str!(
            "../../test-fixtures/pressing-vocabulary/discogs-format-descriptions.txt"
        ))
    );
}

/// Each detail is one description of the list, printed as Discogs prints
/// it, and every description of the list that is a detail is one.
#[test]
fn every_detail_is_one_description() {
    let details: Vec<DiscogsDetail> = DISCOGS_FORMAT_DESCRIPTIONS
        .iter()
        .filter_map(|(term, role)| match role {
            Role::Detail(detail) => {
                assert_eq!(detail.term(), *term);
                Some(*detail)
            }
            _ => None,
        })
        .collect();
    assert_eq!(details, DiscogsDetail::ALL);
}

#[test]
fn a_description_is_read_whole_and_in_any_case() {
    assert_eq!(role("reissue"), Some(Role::Detail(DiscogsDetail::Reissue)));
    assert_eq!(role("Promo"), Some(Role::Status(ReleaseStatus::Promotion)));
    assert_eq!(role("Album"), Some(Role::ReleaseType));
    assert_eq!(role("DualDisc"), Some(Role::HybridCarrier(Medium::DualDisc)));
    assert_eq!(role("180 Gram"), None);
}
