//! The ids of rows whose identity is a fact, not an event.
//!
//! A row that names a catalog entity (an artist, a work, a release group's
//! album) or a relation between rows (a work's credit, a release's record in
//! one catalog) is the fact it states: one catalog id is one row, and the
//! table's natural `UNIQUE` key is that fact. Each id here is a name-based
//! UUID over a text key that spells the fact, so it stays an opaque UUID to
//! everything that stores or shows it. These tables are declared
//! `RowIdentity::SharedKey` in
//! `synced::synced_tables`.

use bae_catalogs::catalog::Catalog;
use bae_catalogs::records::ReleaseRecord;

/// bae's namespace for its name-based row ids.
const NAMESPACE: uuid::Uuid = uuid::Uuid::from_u128(0x6d1f_42c8_9a0b_4e57_b3c1_58f0_2a7d_91e4);

fn id_for(key: &str) -> String {
    uuid::Uuid::new_v5(&NAMESPACE, key.as_bytes()).to_string()
}

/// The artist a catalog names, preferring MusicBrainz. `None` for an artist
/// known only by name: no catalog id names it, so it gets an independent id.
pub fn artist_id(
    musicbrainz_artist_id: Option<&str>,
    discogs_artist_id: Option<&str>,
) -> Option<String> {
    match (musicbrainz_artist_id, discogs_artist_id) {
        (Some(musicbrainz), _) => Some(id_for(&format!("musicbrainz artist {musicbrainz}"))),
        (None, Some(discogs)) => Some(id_for(&format!("discogs artist {discogs}"))),
        (None, None) => None,
    }
}

/// The label a catalog names, preferring MusicBrainz. `None` for a label
/// known only by name: no catalog id names it, so it gets an independent id.
pub fn label_id(
    musicbrainz_label_id: Option<&str>,
    discogs_label_id: Option<&str>,
) -> Option<String> {
    match (musicbrainz_label_id, discogs_label_id) {
        (Some(musicbrainz), _) => Some(id_for(&format!("musicbrainz label {musicbrainz}"))),
        (None, Some(discogs)) => Some(id_for(&format!("discogs label {discogs}"))),
        (None, None) => None,
    }
}

/// The work a MusicBrainz work id names.
pub fn work_id(musicbrainz_work_id: &str) -> String {
    id_for(&format!("musicbrainz work {musicbrainz_work_id}"))
}

/// The album of the group `records` name, preferring the MusicBrainz release
/// group over the Discogs master. `None` when no record names a group.
pub fn album_id_for_records(records: &[ReleaseRecord]) -> Option<String> {
    let groups = records.iter().filter_map(ReleaseRecord::album_ref).collect::<Vec<_>>();
    [Catalog::MusicBrainz, Catalog::Discogs]
        .into_iter()
        .find_map(|catalog| groups.iter().find(|group| group.catalog == catalog))
        .map(|group| id_for(&format!("{} album {}", group.catalog.as_str(), group.key)))
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
#[path = "identity_tests.rs"]
mod tests;
