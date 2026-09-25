//! The ids of rows whose identity is a fact, not an event.
//!
//! coven merges two devices' rows that share `(table, id)`. A row that states a
//! relation between rows (an album credit, a release's record in one catalog)
//! must therefore get the same id on every device that writes the same fact,
//! or two devices writing it while apart collide on the table's natural
//! `UNIQUE` key and the pull that carries the second can never apply. Each id
//! here is a name-based UUID over a text key that spells the fact, so it stays
//! an opaque UUID to everything that stores or shows it. These tables are
//! declared `RowIdentity::SharedKey` in `sync::synced_tables`.

use crate::import::Catalog;

/// bae's namespace for its name-based row ids.
const NAMESPACE: uuid::Uuid = uuid::Uuid::from_u128(0x6d1f_42c8_9a0b_4e57_b3c1_58f0_2a7d_91e4);

fn id_for(key: &str) -> String {
    uuid::Uuid::new_v5(&NAMESPACE, key.as_bytes()).to_string()
}

/// One artist's credit on one album.
pub fn album_artist_id(album_id: &str, artist_id: &str) -> String {
    id_for(&format!("album artist {album_id} {artist_id}"))
}

/// A release's record in one catalog.
pub fn release_record_id(release_id: &str, catalog: Catalog) -> String {
    id_for(&format!("release record {release_id} {}", catalog.as_str()))
}

/// One artist's credit at one position on a work.
pub fn work_artist_id(work_id: &str, artist_id: &str, position: i32) -> String {
    id_for(&format!("work artist {work_id} {artist_id} {position}"))
}

/// A work's membership in a larger work.
pub fn work_part_id(parent_work_id: &str, child_work_id: &str) -> String {
    id_for(&format!("work part {parent_work_id} {child_work_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fact_has_one_id_and_different_facts_do_not_share_one() {
        assert_eq!(album_artist_id("a", "b"), album_artist_id("a", "b"));
        assert_ne!(album_artist_id("a", "b"), album_artist_id("b", "a"));
        assert_ne!(
            release_record_id("r", Catalog::MusicBrainz),
            release_record_id("r", Catalog::Discogs)
        );
    }
}
