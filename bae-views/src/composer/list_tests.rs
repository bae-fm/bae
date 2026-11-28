//! The plans of the composer list's windows, from a key and from the top, one
//! statement for each term a sort can lead with.

use crate::composer::top_reads::{
    ComposerTopByNameDescendingParams, ComposerTopByNameParams,
    ComposerTopByReleasesDescendingParams, ComposerTopByReleasesParams,
    ComposerTopByWorksDescendingParams, ComposerTopByWorksParams,
};
use crate::composer::window_reads::{
    ComposerWindowByNameDescendingParams, ComposerWindowByNameParams,
    ComposerWindowByReleasesDescendingParams, ComposerWindowByReleasesParams,
    ComposerWindowByWorksDescendingParams, ComposerWindowByWorksParams,
};
use bae_store::test_fixtures::query_plan::{plan_steps, PlanOf};

/// Each window statement, the index leading with its first term, and that
/// term's column.
const COMPOSER_WINDOWS: [(PlanOf, &str, &str); 6] = [
    (plan_steps::<ComposerWindowByNameParams>, "idx_composer_listing_name", "name_sort_key"),
    (
        plan_steps::<ComposerWindowByNameDescendingParams>,
        "idx_composer_listing_name_back",
        "sort_name_back",
    ),
    (plan_steps::<ComposerWindowByWorksParams>, "idx_composer_listing_works", "work_count"),
    (
        plan_steps::<ComposerWindowByWorksDescendingParams>,
        "idx_composer_listing_works_back",
        "sort_works_back",
    ),
    (
        plan_steps::<ComposerWindowByReleasesParams>,
        "idx_composer_listing_releases",
        "linked_release_count",
    ),
    (
        plan_steps::<ComposerWindowByReleasesDescendingParams>,
        "idx_composer_listing_releases_back",
        "sort_releases_back",
    ),
];

#[tokio::test]
async fn every_composer_window_seeks_the_index_leading_with_its_first_term() {
    crate::tests::assert_windows_seek_their_first_term(&COMPOSER_WINDOWS).await;
}

#[tokio::test]
async fn every_composer_window_from_the_top_reads_the_index_leading_with_its_first_term() {
    crate::tests::assert_tops_read_their_first_term(&[
        (plan_steps::<ComposerTopByNameParams>, "idx_composer_listing_name"),
        (plan_steps::<ComposerTopByNameDescendingParams>, "idx_composer_listing_name_back"),
        (plan_steps::<ComposerTopByWorksParams>, "idx_composer_listing_works"),
        (plan_steps::<ComposerTopByWorksDescendingParams>, "idx_composer_listing_works_back"),
        (plan_steps::<ComposerTopByReleasesParams>, "idx_composer_listing_releases"),
        (plan_steps::<ComposerTopByReleasesDescendingParams>, "idx_composer_listing_releases_back"),
    ])
    .await;
}

/// A window from the top holds the list's first rows under every sort.
#[tokio::test]
async fn every_composer_sorts_window_from_the_top_holds_its_first_rows() {
    use crate::{ComposerSortCriterion, ComposerSortField};
    let (db, _tmp) = crate::tests::library().await;
    db.read(|sql| {
        for sort in crate::tests::sorts_of(&[
            ComposerSortField::Name,
            ComposerSortField::WorkCount,
            ComposerSortField::LinkedReleaseCount,
        ]) {
            let criteria: Vec<_> = sort
                .iter()
                .map(|&(field, direction)| ComposerSortCriterion { field, direction })
                .collect();
            let sort = crate::composer::composer_sort(&criteria);
            crate::tests::assert_top_is_first_rows(
                &format!("{criteria:?}"),
                || crate::composer::composer_key_at_on(&sql, &sort, 0),
                |window| crate::composer::composer_window_on(&sql, &sort, window),
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
}
