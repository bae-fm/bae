//! bae's sync boundary around coven.
//!
//! coven owns the sync engine, cloud homes, keys, storage, and the DB
//! connection. bae's side of that boundary lives here: coven's join/restore entry
//! points re-exported at `crate::sync`, the upload observer that turns coven blob
//! transitions into UI events, the membership types the join/invite screens use,
//! the synced-table declarations, and bae's blob namespaces plus cache budgets.

// The sync substrate lives in coven; these resolve `crate::sync::<item>`
// unchanged. Blob-key derivation is coven's, reached through
// `CovenHandle::blob_cloud_key`.
pub use coven::{decode_restore_code_info, restore_from_code, RestoreSource};

// `CloudCipher` is what a test hands to `connect_sync_with_test_home`; coven
// only compiles it under `test`/`test-utils`, so bae's re-export follows.
#[cfg(any(test, feature = "test-utils"))]
pub use coven::CloudCipher;

// bae's blob-transition observer: UI bookkeeping for coven's upload drain and
// make-Remote / make-Local completions (coven owns the lifecycle itself).
pub mod upload_observer;

pub mod membership;

use coven::{BlobDecl, RowIdentity, SyncedTable};
use coven::{CacheFill, Provenance};

/// The localized message key for a visible post-open artwork-loading state.
/// Terminal success and the idle state have no banner.
pub fn eager_cache_fill_title_key(status: &coven::EagerCacheFillStatus) -> Option<&'static str> {
    match status {
        coven::EagerCacheFillStatus::NotRunning | coven::EagerCacheFillStatus::Complete { .. } => {
            None
        }
        coven::EagerCacheFillStatus::Scanning => Some("core.artwork_cache.scanning"),
        coven::EagerCacheFillStatus::Downloading(_) => Some("core.artwork_cache.downloading"),
        coven::EagerCacheFillStatus::Cancelled(_) => Some("core.artwork_cache.cancelled"),
        coven::EagerCacheFillStatus::Failed { .. } => Some("core.artwork_cache.failed"),
    }
}

/// S3 configuration data for save_s3_config.
pub struct S3ConfigData {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub key_prefix: Option<String>,
    pub access_key: String,
    pub secret_key: String,
    /// Opaque (encrypted, obfuscated) or browsable (plaintext, readable) home.
    pub storage: crate::config::HomeStorage,
}

/// Cloud namespace for release-file (audio/image/text/…) blobs — the user's own
/// imported files. coven keys them under `release_files/…` and segments their
/// cache + budget by this name.
pub const RELEASE_FILES_NAMESPACE: &str = "release_files";
/// Cloud namespace for the bae-produced album cover blob (1:1 with a release).
pub const COVERS_NAMESPACE: &str = "covers";
/// Cloud namespace for the bae-produced artist image blob (1:1 with an artist).
pub const ARTIST_IMAGES_NAMESPACE: &str = "artist_images";

/// This device's cache budget (bytes) for Remote `release_files` blobs — the bulk
/// of the cache, since audio dominates. Each namespace evicts against its own
/// budget, so audio pressure never wipes the cover cache.
pub const RELEASE_FILES_CACHE_BUDGET: u64 = 20 * 1024 * 1024 * 1024; // 20 GiB
/// The reserved cache budget for Remote `covers` blobs (grid art). A `CacheEager`
/// cover evicted under pressure shows a placeholder and re-fetches on the next
/// pull — covers are not pinned.
pub const COVERS_CACHE_BUDGET: u64 = 512 * 1024 * 1024; // 512 MiB
/// The reserved cache budget for Remote `artist_images` blobs.
pub const ARTIST_IMAGES_CACHE_BUDGET: u64 = 256 * 1024 * 1024; // 256 MiB

/// The tables coven captures into changesets for incremental sync.
///
/// coven only attaches tables it is told about — a row in an unregistered table
/// never propagates — applies changesets keyed on the column-0 PRIMARY KEY, and
/// resolves conflicts last-writer-wins on `_updated_at`. So a table syncs only
/// if it has both an `id TEXT PRIMARY KEY` at column 0 and an
/// `_updated_at TEXT NOT NULL`. Every table below has both (the tests here check
/// that against `bae-core/migrations/001_initial.sql`).
///
/// ## The `releases.remote` gate
///
/// `releases` is the only *gated root*: a row syncs only when its `remote` column
/// is true, and the gate flows down the declared foreign keys, so a descendant
/// syncs iff its root release is remote. The children — `tracks`, `track_artists`
/// (2-hop, via `tracks`), `release_files`, `release_identities`, `audio_formats`,
/// `audio_format_segments`, and the `covers` asset — are declared plain (or, for
/// the cover, an `.asset()`) and pick the gate up from coven's FK walk, not from
/// a per-table flag. Flipping `remote` true re-emits the whole now-visible
/// subtree to peers as full inserts.
///
/// `albums`, `artists`, and `works` are FK-ancestors of `releases`, declared
/// `gated_by_descendants()`: an album syncs only while it has a surviving
/// (remote) release, and an artist syncs only while a surviving album,
/// `album_artists`, or `track_artists` row references it. coven infers those
/// keep-children from the FK graph, so a receiver never materializes an album
/// with zero remote releases and there is no read-side filter to hide one.
/// `album_artists`, `work_artists`, `work_parts`, `track_works`, and the
/// `*_artist_roles` tables are plain join tables that ride along.
///
/// `release_files` carries the user's own imported-file blobs; `covers` and
/// `artist_images` carry bae-produced image blobs and are `.asset()`s of their
/// FK subject (a release / an artist), so they ride the subject's gate but never
/// keep it alive. coven owns the whole blob lifecycle off these declarations
/// (upload/download, the make-Remote/make-Local transitions, the locality-aware
/// read), so bae hand-maintains no blob source.
///
/// Excluded: the device-local tables (`source_release_payloads`,
/// `playback_state`, `import_candidate_state`) have no `_updated_at`, and
/// coven's own bookkeeping tables live outside bae's migration entirely — bae
/// never names them.
///
/// Passed to [`coven::Coven::builder`], which attaches the capture session to
/// exactly these tables when the library is opened.
pub fn synced_tables() -> Vec<SyncedTable> {
    vec![
        SyncedTable::new("artists", RowIdentity::IndependentUuid).gated_by_descendants(),
        SyncedTable::new("albums", RowIdentity::IndependentUuid).gated_by_descendants(),
        SyncedTable::new("album_artists", RowIdentity::IndependentUuid),
        SyncedTable::new("releases", RowIdentity::IndependentUuid).gated_by("remote"),
        SyncedTable::new("release_identities", RowIdentity::IndependentUuid),
        SyncedTable::new("tracks", RowIdentity::IndependentUuid),
        SyncedTable::new("track_artists", RowIdentity::IndependentUuid),
        SyncedTable::new("works", RowIdentity::IndependentUuid).gated_by_descendants(),
        SyncedTable::new("work_artists", RowIdentity::IndependentUuid),
        SyncedTable::new("work_parts", RowIdentity::IndependentUuid),
        SyncedTable::new("track_works", RowIdentity::IndependentUuid),
        SyncedTable::new("release_artist_roles", RowIdentity::IndependentUuid),
        SyncedTable::new("track_artist_roles", RowIdentity::IndependentUuid),
        // The user's own imported files: user-provided (Local = the file at the
        // user's path, an external ref coven holds), CacheLazy (fetched on first
        // read when Remote). coven reads the blob id off the PK and the readable
        // cloud key off `cloud_path`. write_once: the row is never repointed — a
        // re-import mints a new release id, hence a new blob and path, so an
        // audio object at a key never changes content. That is what lets the
        // cloud key stay a readable name with no blob id in it; coven refuses a
        // repoint rather than silently rewriting an object a peer already holds.
        SyncedTable::new("release_files", RowIdentity::IndependentUuid).carries_blob(
            BlobDecl::new(
                RELEASE_FILES_NAMESPACE,
                Provenance::UserProvided,
                CacheFill::CacheLazy,
            )
            .with_size_column("file_size")
            .with_cloud_path_column("cloud_path")
            .write_once(),
        ),
        SyncedTable::new("audio_formats", RowIdentity::IndependentUuid),
        SyncedTable::new("audio_format_segments", RowIdentity::IndependentUuid),
        // The bae-produced album cover: host-provided (coven owns the copy in
        // its local store while Local), CacheEager (pulled with the row when
        // Remote so the grid renders from local bytes). An asset — it rides its
        // release's gate (FK on `id`) but never keeps the release alive.
        //
        // The blob id comes from `blob_id`, not the PK: the PK is the release id
        // and cannot move, while a coven blob id names one immutable byte-string.
        // Changing the cover mints a new `blob_id` and deletes the old blob.
        SyncedTable::new("covers", RowIdentity::IndependentUuid)
            .carries_blob(
                BlobDecl::new(
                    COVERS_NAMESPACE,
                    Provenance::HostProvided,
                    CacheFill::CacheEager,
                )
                .with_id_column("blob_id")
                .with_size_column("file_size")
                .with_cloud_path_column("cloud_path"),
            )
            .asset(),
        // The bae-produced artist image, same shape, riding `artists`' gate.
        SyncedTable::new("artist_images", RowIdentity::IndependentUuid)
            .carries_blob(
                BlobDecl::new(
                    ARTIST_IMAGES_NAMESPACE,
                    Provenance::HostProvided,
                    CacheFill::CacheEager,
                )
                .with_id_column("blob_id")
                .with_size_column("file_size")
                .with_cloud_path_column("cloud_path"),
            )
            .asset(),
    ]
}

/// The coven [`BlobRef`](coven::BlobRef) for a host-provided library image (a cover
/// or an artist image) — its identity in coven's local store while Local and its
/// cache while Remote. `namespace` is [`COVERS_NAMESPACE`] or
/// [`ARTIST_IMAGES_NAMESPACE`]; `blob_id` is the image row's `blob_id`, NOT the row
/// id (the release/artist id). A host-provided `CacheEager` blob: the bytes are
/// produced by bae and kept by coven, fetched into the cache on pull so a grid
/// renders from local bytes. `cloud_path` is the row's readable path on a browsable
/// home (`None` on an opaque one).
pub(crate) fn image_blob_ref(
    namespace: &str,
    blob_id: &str,
    cloud_path: Option<String>,
) -> coven::BlobRef {
    coven::BlobRef {
        namespace: namespace.to_string(),
        id: blob_id.to_string(),
        scope: coven::BlobScope::Master,
        cloud_path,
        provenance: coven::Provenance::HostProvided,
        fill: coven::CacheFill::CacheEager,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::synced_tables;

    /// `(table_name, column_body)` for every `CREATE TABLE` in the migrations. The
    /// body is delimited by depth-matched parens, so a nested `CHECK (...)` doesn't
    /// truncate it.
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

        // Every table is `CREATE TABLE IF NOT EXISTS` — the migration is
        // idempotent so coven can re-run it over a snapshot-bootstrapped DB.
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

    /// The synced set must be exactly the tables carrying an `_updated_at` clock —
    /// no more, no fewer. A device-local table (`playback_state`) that grew an
    /// `_updated_at` would start leaking per-device state across devices; a new
    /// synced table left off the registration would silently never propagate.
    /// Either drift breaks this test.
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

    /// `releases` is the only gated root, gated by `remote`; its subtree inherits
    /// the gate via coven's FK walk and is declared plain. A gate accidentally
    /// attached to a subtree table — or to a gated-by-descendants ancestor like
    /// `albums` — would diverge from the schema's FK shape, which this catches.
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

    /// `albums`, `artists`, and `works` are the FK-ancestors of the gated
    /// `releases` root, declared `gated_by_descendants()` so a row drops out of
    /// sync once its gated subtree empties. Any other table carrying the marker —
    /// or one of these losing it — would diverge from the schema's FK shape.
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

    /// `import_candidate_state` holds per-device identify verdicts and retry
    /// state; syncing it would leak one device's retry bookkeeping to every
    /// other device on the account. A dedicated assertion, not just reliance on
    /// `synced_tables_equal_the_lww_clock_set`: that test only catches the two
    /// sides *disagreeing* — it stays green if someone gives this table an
    /// `_updated_at` column and registers it in `synced_tables()` together,
    /// since then both sides would agree it belongs in the synced set. This
    /// test catches that case specifically, by naming the table regardless of
    /// whether it ever grows a clock column.
    #[test]
    fn import_candidate_state_is_not_synced() {
        let synced = synced_tables();
        let registered: BTreeSet<&str> = synced.iter().map(|t| t.name()).collect();
        assert!(
            !registered.contains("import_candidate_state"),
            "import_candidate_state is device-local and must never sync"
        );
    }

    /// `source_release_payloads` holds re-fetchable provider documents, not the
    /// user's library: syncing them would push megabytes of MusicBrainz and
    /// Discogs JSON to every device to save each of them a lookup it can make
    /// itself. Named for the same reason `import_candidate_state` is — so the
    /// exclusion survives the table growing a clock column.
    #[test]
    fn source_release_payloads_are_not_synced() {
        let synced = synced_tables();
        let registered: BTreeSet<&str> = synced.iter().map(|t| t.name()).collect();
        assert!(
            !registered.contains("source_release_payloads"),
            "source_release_payloads is device-local and must never sync"
        );
    }

    #[test]
    fn device_local_import_folder_tables_are_not_synced() {
        let tables = synced_tables();
        let registered: BTreeSet<&str> = tables.iter().map(|table| table.name()).collect();
        for table in [
            "watched_import_folders",
            "skipped_import_candidates",
            "folder_release_decisions",
            "folder_scan_generation_sequence",
            "folder_scan_roots",
            "scan_candidate",
            "scan_candidate_file",
            "scan_cue_sheet",
            "scan_cue_track",
            "scan_cue_index",
            "scan_candidate_resolved_boundary",
            "import_candidate_state",
            "import_candidate_match",
            "import_candidate_file_edit",
            "import_candidate_file_duration",
            "import_candidate_signals",
            "import_candidate_signal_value",
            "import_candidate_failure",
            "import_candidate_cover",
            "import_candidate_edit",
            "import_candidate_track_edit",
        ] {
            assert!(
                !registered.contains(table),
                "{table} holds device-local import scan state"
            );
        }
    }

    /// A blob declaration or asset flag silently dropped from one of these tables
    /// would break the whole coven-owned blob lifecycle.
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
