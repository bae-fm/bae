//! The plans of the composer list's windows, from a key and from the top, one
//! statement for each term a sort can lead with.

use crate::composer::top_reads::{
    ComposerTopByName, ComposerTopByNameDescending, ComposerTopByReleases,
    ComposerTopByReleasesDescending, ComposerTopByWorks, ComposerTopByWorksDescending,
};
use crate::composer::window_reads::{
    ComposerWindowByName, ComposerWindowByNameDescending, ComposerWindowByReleases,
    ComposerWindowByReleasesDescending, ComposerWindowByWorks, ComposerWindowByWorksDescending,
};

/// Each window statement, the index leading with its first term, and that
/// term's column.
const COMPOSER_WINDOWS: [(&str, &str, &str); 6] = [
    (ComposerWindowByName::SQL, "idx_composer_listing_name", "name_sort_key"),
    (ComposerWindowByNameDescending::SQL, "idx_composer_listing_name_back", "sort_name_back"),
    (ComposerWindowByWorks::SQL, "idx_composer_listing_works", "work_count"),
    (ComposerWindowByWorksDescending::SQL, "idx_composer_listing_works_back", "sort_works_back"),
    (ComposerWindowByReleases::SQL, "idx_composer_listing_releases", "linked_release_count"),
    (
        ComposerWindowByReleasesDescending::SQL,
        "idx_composer_listing_releases_back",
        "sort_releases_back",
    ),
];

#[tokio::test]
async fn every_composer_window_seeks_the_index_leading_with_its_first_term() {
    crate::tests::assert_windows_seek_their_first_term(&COMPOSER_WINDOWS, 3).await;
}

#[tokio::test]
async fn every_composer_window_from_the_top_reads_the_index_leading_with_its_first_term() {
    crate::tests::assert_tops_read_their_first_term(&[
        (ComposerTopByName::SQL, "idx_composer_listing_name"),
        (ComposerTopByNameDescending::SQL, "idx_composer_listing_name_back"),
        (ComposerTopByWorks::SQL, "idx_composer_listing_works"),
        (ComposerTopByWorksDescending::SQL, "idx_composer_listing_works_back"),
        (ComposerTopByReleases::SQL, "idx_composer_listing_releases"),
        (ComposerTopByReleasesDescending::SQL, "idx_composer_listing_releases_back"),
    ])
    .await;
}
