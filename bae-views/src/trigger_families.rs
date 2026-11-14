//! The families of triggers the library views keep their listing tables with,
//! each dropped from a closed library file by a name, to time what a write
//! costs without it.

use bae_store::test_fixtures::planted_damage::drop_triggers_of;
use std::path::Path;

/// The album grid's `album_listing` and `album_artist_link`.
pub fn drop_album_listing_triggers(directory: &Path) {
    drop_triggers_of(directory, "album_listing", crate::album::LISTING_SQL);
}

/// The Storage Manager's `release_listing`.
pub fn drop_release_listing_triggers(directory: &Path) {
    drop_triggers_of(directory, "release_listing", crate::storage::SQL);
}

/// The composer list's `composer_work`, `composer_release`,
/// `composer_unlinked_credit` and `composer_listing`.
pub fn drop_composer_triggers(directory: &Path) {
    drop_triggers_of(directory, "composer_listing", crate::composer::SQL);
}

/// The composer page's `composer_detail_row`.
pub fn drop_composer_detail_triggers(directory: &Path) {
    drop_triggers_of(directory, "composer_detail_row", crate::composer_detail::SQL);
}

/// The library search's full-text tables.
pub fn drop_search_triggers(directory: &Path) {
    drop_triggers_of(directory, "album_search_entry", crate::search::SQL);
}

/// The artist list's `artist_listing`.
pub fn drop_artist_listing_triggers(directory: &Path) {
    drop_triggers_of(directory, "artist_listing", crate::artist::SQL);
}
