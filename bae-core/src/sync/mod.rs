//! bae's sync: coven's sync substrate re-exported, with bae's blob source and a
//! `SyncManager` wrapper layered on top.

// The sync substrate lives in coven; these resolve `crate::sync::<m>` unchanged.
pub use coven::join_code;
pub use coven::sync::{cloud_storage, join, restore, restore_code};

// Blob-key construction at bae's call sites: `CloudSyncStorage::blob_key` keyed
// by a `BlobPathScheme`. An opaque home is `Hashed` (keyed by the id), a
// browsable home is `Plain` (the row's readable `cloud_path` verbatim).
pub use coven::sync::cloud_storage::{BlobPathScheme, CloudSyncStorage};

// bae-only domain layers built on the substrate.
pub mod blob_source;

// bae's SyncManager wrapper owns and drives coven's SyncManager.
pub mod sync_manager;

use coven::sync::session::SyncedTable;

/// The tables coven captures into changesets for incremental sync.
///
/// coven's SQLite session only attaches tables it's been told about; everything
/// else is invisible to changeset capture, and a row in an unregistered table
/// never propagates. coven applies changesets keyed on the column-0 PRIMARY KEY,
/// and resolves conflicts last-writer-wins on `_updated_at`. So a table may sync
/// only if it has BOTH:
///   - an `id TEXT PRIMARY KEY` at column 0 (the apply key), and
///   - an `_updated_at TEXT NOT NULL` column (the LWW clock).
///
/// These ten satisfy both (verified against
/// `bae-core/migrations/001_initial.sql`).
///
/// ## The `releases.remote` gate
///
/// `releases` is a *gated root*: a row syncs only when its `remote` column is
/// true, and the gate flows down the declared foreign keys to the release
/// subtree, so its descendants sync iff their root release is remote. The
/// inheriting children — `tracks`, `track_artists` (2-hop, via `tracks`),
/// `release_files`, `release_identities`, `audio_formats` — are declared plain;
/// they pick up the gate automatically from coven's FK walk, not from a
/// per-table flag. Flipping `remote` true re-emits the whole now-visible
/// subtree to peers as full inserts.
///
/// `albums` and `artists` are FK-ancestors of `releases`, declared
/// `gated_by_descendants()`: an album syncs only while it has a surviving
/// (remote) release, and an artist syncs only while a surviving album,
/// `album_artists`, or `track_artists` row references it. coven infers those
/// keep-children from the foreign-key graph, so a receiver never materializes an
/// album with zero remote releases and there is no read-side filter to hide
/// one. `album_artists` is a plain join table that rides along.
///
/// Deliberately excluded:
///   - local-only tables (`release_metadata`, `imports`, `attribution_names`)
///     — no `_updated_at`. coven's own bookkeeping tables (`sync_state`,
///     `sync_cursors`, `cloud_outbox`) live outside bae's migration entirely.
///
/// Passed to [`coven::Database::open`], which attaches the capture session to
/// exactly these tables and owns the set thereafter (read back via
/// `coven::Database::synced_tables`).
pub fn synced_tables() -> Vec<SyncedTable> {
    vec![
        SyncedTable::new("artists").gated_by_descendants(),
        SyncedTable::new("albums").gated_by_descendants(),
        SyncedTable::new("album_artists"),
        SyncedTable::new("releases").gated_by("remote"),
        SyncedTable::new("release_identities"),
        SyncedTable::new("tracks"),
        SyncedTable::new("track_artists"),
        SyncedTable::new("release_files"),
        SyncedTable::new("audio_formats"),
        SyncedTable::new("library_images"),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::synced_tables;

    /// `(table_name, column_body)` for every `CREATE TABLE` in the migrations,
    /// with the body delimited by depth-matched parens (so nested `CHECK (...)`
    /// constraints don't truncate it).
    fn migration_tables() -> Vec<(String, String)> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .expect("migrations dir")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "sql"))
            .collect();
        paths.sort();
        let sql = paths
            .iter()
            .map(|p| std::fs::read_to_string(p).expect("read migration"))
            .collect::<Vec<_>>()
            .join("\n");

        // The migration is idempotent — every table is `CREATE TABLE IF NOT
        // EXISTS` so coven can re-run it over a snapshot-bootstrapped DB.
        let marker = "CREATE TABLE IF NOT EXISTS ";
        let mut out = Vec::new();
        let mut cursor = 0;
        while let Some(rel) = sql[cursor..].find(marker) {
            let after = cursor + rel + marker.len();
            let name: String = sql[after..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let open = after + sql[after..].find('(').expect("table has a column list");
            let mut depth = 0usize;
            let mut close = open;
            for (k, ch) in sql[open..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = open + k;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            out.push((name, sql[open + 1..close].to_string()));
            cursor = close;
        }
        out
    }

    fn has_lww_clock(body: &str) -> bool {
        body.lines()
            .any(|l| l.trim_start().starts_with("_updated_at"))
    }

    fn id_pk_at_column_0(body: &str) -> bool {
        let first = body
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with("--"))
            .unwrap_or("")
            .to_ascii_lowercase();
        first.starts_with("id ") && first.contains("primary key")
    }

    /// The synced set must be exactly the tables carrying an `_updated_at` LWW
    /// clock — no more, no fewer. A device-local table (e.g. `release_local_copy`)
    /// that grew an `_updated_at` would start leaking per-device state across
    /// devices; a new synced table left off the registration would silently
    /// never propagate. Either drift breaks this test.
    #[test]
    fn synced_tables_equal_the_lww_clock_set() {
        let tables = migration_tables();
        assert!(
            tables.len() > 10,
            "table parser under-counted: {}",
            tables.len()
        );

        let with_clock: BTreeSet<&str> = tables
            .iter()
            .filter(|(_, body)| has_lww_clock(body))
            .map(|(name, _)| name.as_str())
            .collect();
        let synced = synced_tables();
        let registered: BTreeSet<&str> = synced.iter().map(|t| t.name()).collect();
        assert_eq!(
            registered, with_clock,
            "the synced set must equal the set of tables with an `_updated_at` clock"
        );

        // coven keys changeset apply on the column-0 PK, so every synced table
        // needs `id TEXT PRIMARY KEY` first.
        for (name, body) in &tables {
            if registered.contains(name.as_str()) {
                assert!(
                    id_pk_at_column_0(body),
                    "synced table `{name}` must have `id` PRIMARY KEY at column 0"
                );
            }
        }
    }

    /// `releases` is the only gated root, gated by `remote`. The release subtree
    /// (`tracks`, `track_artists`, `release_files`, `release_identities`,
    /// `audio_formats`) inherits the gate via coven's FK walk and so is declared
    /// plain; a gate accidentally attached to one of those — or to a
    /// gated-by-descendants ancestor like `albums` — would diverge from the
    /// schema's FK shape and is what this test catches.
    #[test]
    fn releases_is_the_only_gated_root() {
        for table in synced_tables() {
            let expected_gate = (table.name() == "releases").then_some("remote");
            assert_eq!(
                table.gate_column(),
                expected_gate,
                "unexpected gate on `{}`",
                table.name()
            );
        }
    }

    /// `albums` and `artists` are the FK-ancestors of the gated `releases` root,
    /// declared `gated_by_descendants()` so a row drops out of sync once its gated
    /// subtree empties (coven infers the keep-children from the FK graph). Any
    /// other table carrying this marker — or one of these two losing it — would
    /// diverge from the schema's FK shape, which is what this test catches.
    #[test]
    fn albums_and_artists_are_the_gated_by_descendants_ancestors() {
        let tables = synced_tables();
        let ancestors: BTreeSet<&str> = tables
            .iter()
            .filter(|t| t.is_gated_by_descendants())
            .map(|t| t.name())
            .collect();
        assert_eq!(ancestors, BTreeSet::from(["albums", "artists"]));
    }

    /// The whole point of the storage-state split: per-device local-source
    /// state lives in `release_local_source`, which must never sync.
    #[test]
    fn release_local_source_is_device_local() {
        let tables = migration_tables();
        let (_, body) = tables
            .iter()
            .find(|(n, _)| n == "release_local_source")
            .expect("release_local_source table exists");
        assert!(
            !has_lww_clock(body),
            "release_local_source must have no `_updated_at` (device-local, never synced)"
        );
        assert!(synced_tables()
            .iter()
            .all(|t| t.name() != "release_local_source"));
    }
}
