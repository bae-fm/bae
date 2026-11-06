//! A library-sized release for benchmarks: more tracks than coven's writes
//! put in within a run.

/// Put `count` tracks with random ids on `release_id`'s first medium, in the
/// SQLite file at `database`, through a plain connection with triggers off.
/// Through coven every synced row's write is captured whole, which takes
/// minutes for a million rows; a benchmark that reads nothing of `tracks` but
/// its rows needs only the rows.
pub fn insert_with_triggers_off(database: &std::path::Path, release_id: &str, count: i64) {
    let plain = rusqlite::Connection::open(database).unwrap();
    plain.busy_timeout(std::time::Duration::from_secs(60)).unwrap();
    plain.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false).unwrap();
    plain
        .execute(
            &format!(
                "WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < {count} - 1) \
                 INSERT INTO tracks \
                     (position, id, release_id, title, medium_id, side, track_number, duration_ms, created_at) \
                 SELECT i, \
                        lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' \
                              || substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(2)) \
                              || '-' || hex(randomblob(6))), \
                        ?1, 'Track Title ' || i, ?2, 1, i + 1, 1000, '2026-01-01T00:00:00Z' \
                 FROM n"
            ),
            rusqlite::params![release_id, crate::identity::release_medium_id(release_id, 1)],
        )
        .unwrap();
}
