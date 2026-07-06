//! bae's sync: coven's sync substrate re-exported, with bae's blob-transition
//! observer and a `SyncManager` wrapper layered on top.

// The sync substrate lives in coven; these resolve `crate::sync::<item>`
// unchanged. `CloudCipher` is what a test hands to `connect_sync_with_test_home`;
// blob-key derivation is coven's, reached through `CovenHandle::blob_cloud_key`.
pub use coven::{
    decode_invite_code_info, decode_restore_code_info, join_from_invite_code, restore_from_cloud,
    restore_from_code, CloudCipher, RestoreSource,
};

// bae's blob-transition observer: UI bookkeeping for coven's upload drain and
// make-Remote / make-Local completions (coven owns the lifecycle itself).
pub mod upload_observer;

// bae's SyncManager wrapper owns and drives coven's SyncManager.
pub mod sync_manager;

use coven::{BlobDecl, SyncedTable};
use coven::{CacheFill, Provenance};

/// Cloud namespace for release-file (audio/image/text/…) blobs — the user's own
/// imported files. coven keys them under `release_files/…` and segments their
/// cache + budget by this name.
pub const RELEASE_FILES_NAMESPACE: &str = "release_files";
/// Cloud namespace for the bae-produced album cover blob (1:1 with a release).
pub const COVERS_NAMESPACE: &str = "covers";
/// Cloud namespace for the bae-produced artist image blob (1:1 with an artist).
pub const ARTIST_IMAGES_NAMESPACE: &str = "artist_images";

/// This device's cache-size budget (bytes) for Remote `release_files` blobs —
/// the bulk of the cache, since audio dominates. Each namespace evicts against
/// its own budget, so audio pressure never wipes the cover cache.
pub const RELEASE_FILES_CACHE_BUDGET: u64 = 20 * 1024 * 1024 * 1024; // 20 GiB
/// A small reserved cache budget for Remote `covers` blobs (grid art), evicted
/// independently of audio. A `CacheEager` cover evicted under pressure shows a
/// placeholder and re-fetches on the next pull — covers are not pinned.
pub const COVERS_CACHE_BUDGET: u64 = 512 * 1024 * 1024; // 512 MiB
/// A small reserved cache budget for Remote `artist_images` blobs.
pub const ARTIST_IMAGES_CACHE_BUDGET: u64 = 256 * 1024 * 1024; // 256 MiB

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
/// The set below satisfies both (verified against
/// `bae-core/migrations/001_initial.sql`).
///
/// ## The `releases.remote` gate
///
/// `releases` is a *gated root*: a row syncs only when its `remote` column is
/// true, and the gate flows down the declared foreign keys to the release
/// subtree, so its descendants sync iff their root release is remote. The
/// inheriting children — `tracks`, `track_artists` (2-hop, via `tracks`),
/// `release_files`, `release_identities`, `audio_formats`, and the `covers`
/// asset — are declared plain (or, for the cover, an `.asset()`); they pick up
/// the gate automatically from coven's FK walk, not from a per-table flag.
/// Flipping `remote` true re-emits the whole now-visible subtree to peers as
/// full inserts.
///
/// `albums` and `artists` are FK-ancestors of `releases`, declared
/// `gated_by_descendants()`: an album syncs only while it has a surviving
/// (remote) release, and an artist syncs only while a surviving album,
/// `album_artists`, or `track_artists` row references it. coven infers those
/// keep-children from the foreign-key graph, so a receiver never materializes an
/// album with zero remote releases and there is no read-side filter to hide
/// one. `album_artists` is a plain join table that rides along.
///
/// `release_files` carries the user's own imported-file blobs; `covers` and
/// `artist_images` carry bae-produced image blobs and are `.asset()`s of their
/// FK subject (a release / an artist), so they ride the subject's gate but never
/// keep it alive. coven owns the whole blob lifecycle off these declarations
/// (upload/download, the make-Remote/make-Local transitions, the locality-aware
/// read), so bae does not hand-maintain a blob source.
///
/// Deliberately excluded:
///   - local-only tables (`release_metadata`, `imports`, `attribution_names`)
///     — no `_updated_at`. coven's own bookkeeping tables (`sync_state`,
///     `sync_cursors`, `cloud_outbox`) live outside bae's migration entirely.
///
/// Passed to [`coven::Coven::builder`], which attaches the capture session to
/// exactly these tables when the library is opened.
pub fn synced_tables() -> Vec<SyncedTable> {
    vec![
        SyncedTable::new("artists").gated_by_descendants(),
        SyncedTable::new("albums").gated_by_descendants(),
        SyncedTable::new("album_artists"),
        SyncedTable::new("releases").gated_by("remote"),
        SyncedTable::new("release_identities"),
        SyncedTable::new("tracks"),
        SyncedTable::new("track_artists"),
        SyncedTable::new("works").gated_by_descendants(),
        SyncedTable::new("work_artists"),
        SyncedTable::new("work_parts"),
        SyncedTable::new("track_works"),
        SyncedTable::new("release_artist_roles"),
        SyncedTable::new("track_artist_roles"),
        // The user's own imported files: user-provided (Local = the file at the
        // user's path, an external ref coven holds), CacheLazy (fetched on first
        // read when Remote). coven reads the blob id off the PK and the readable
        // cloud key off `cloud_path`.
        SyncedTable::new("release_files").carries_blob(
            BlobDecl::new(
                RELEASE_FILES_NAMESPACE,
                Provenance::UserProvided,
                CacheFill::CacheLazy,
            )
            .with_cloud_path_column("cloud_path"),
        ),
        SyncedTable::new("audio_formats"),
        SyncedTable::new("audio_format_segments"),
        // The bae-produced album cover: host-provided (coven owns the copy in
        // its local store while Local), CacheEager (pulled with the row when
        // Remote so the grid renders from local bytes). An asset — it rides its
        // release's gate (FK on `id`) but never keeps the release alive.
        SyncedTable::new("covers")
            .carries_blob(
                BlobDecl::new(
                    COVERS_NAMESPACE,
                    Provenance::HostProvided,
                    CacheFill::CacheEager,
                )
                .with_cloud_path_column("cloud_path"),
            )
            .asset(),
        // The bae-produced artist image, same shape, riding `artists`' gate.
        SyncedTable::new("artist_images")
            .carries_blob(
                BlobDecl::new(
                    ARTIST_IMAGES_NAMESPACE,
                    Provenance::HostProvided,
                    CacheFill::CacheEager,
                )
                .with_cloud_path_column("cloud_path"),
            )
            .asset(),
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
    /// `audio_formats`, `audio_format_segments`) inherits the gate via coven's
    /// FK walk and so is declared plain; a gate accidentally attached to one of those — or to a
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

    /// `albums`, `artists`, and `works` are FK-ancestors of the gated `releases`
    /// root, declared `gated_by_descendants()` so a row drops out of sync once its
    /// gated subtree empties (coven infers the keep-children from the FK graph).
    /// Any other table carrying this marker — or one of these losing it — would
    /// diverge from the schema's FK shape, which is what this test catches.
    #[test]
    fn albums_artists_and_works_are_the_gated_by_descendants_ancestors() {
        let tables = synced_tables();
        let ancestors: BTreeSet<&str> = tables
            .iter()
            .filter(|t| t.is_gated_by_descendants())
            .map(|t| t.name())
            .collect();
        assert_eq!(ancestors, BTreeSet::from(["albums", "artists", "works"]));
    }

    /// `release_files` carries the user's own blobs; `covers` and `artist_images`
    /// carry bae-produced image blobs and are assets (ride their FK subject's
    /// gate, never grant keep). A blob declaration or asset flag silently dropped
    /// here would break the whole coven-owned blob lifecycle, which is what this
    /// test catches.
    #[test]
    fn blob_bearing_tables_carry_their_declarations() {
        let tables = synced_tables();
        let by_name = |name: &str| tables.iter().find(|t| t.name() == name).unwrap();

        assert!(by_name("release_files").blob().is_some());
        assert!(!by_name("release_files").is_asset());

        let covers = by_name("covers");
        assert!(covers.blob().is_some(), "covers carries a blob");
        assert!(covers.is_asset(), "covers is an asset of releases");

        let artist_images = by_name("artist_images");
        assert!(
            artist_images.blob().is_some(),
            "artist_images carries a blob"
        );
        assert!(
            artist_images.is_asset(),
            "artist_images is an asset of artists"
        );

        // Non-blob tables carry no declaration.
        assert!(by_name("tracks").blob().is_none());
    }
}
