//! A library-sized release for benchmarks: more tracks than coven's writes
//! put in within a run.

/// The statement that puts `count` tracks with random ids on `release_id`'s
/// first medium. Through coven every synced row's write is captured whole,
/// which takes minutes for a million rows; a benchmark that reads nothing of
/// `tracks` but its rows plants them with this statement instead.
fn insert_sql(release_id: &str, count: i64) -> String {
    format!(
        "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < {count} - 1) \
         INSERT INTO tracks \
             (position, id, release_id, title, medium_id, side, track_number, duration_ms, created_at) \
         SELECT i, \
                lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' \
                      || substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(2)) \
                      || '-' || hex(randomblob(6))), \
                '{release_id}', 'Track Title ' || i, '{medium}', 1, i + 1, 1000, \
                '2026-01-01T00:00:00Z' \
         FROM n",
        medium = crate::first_medium_id(release_id),
    )
}

/// Plant `count` tracks on `release_id`'s first medium in the library under
/// `directory`, outside the store's writes, as `plant_in_library_file` plants
/// damage.
pub fn plant_in_library(directory: &std::path::Path, release_id: &str, count: i64) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "tracks",
        &insert_sql(release_id, count),
    );
}

/// Put `count` tracks on `release_id`'s first medium in `plain`, a connection
/// to a database the caller made itself.
pub fn insert_into(plain: &rusqlite::Connection, release_id: &str, count: i64) {
    plain.execute_batch(&insert_sql(release_id, count)).unwrap();
}
