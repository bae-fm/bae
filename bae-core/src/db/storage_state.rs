//! The single owner of every write to the device storage stores —
//! `releases.managed`, `release_unmanaged_source`, and `file_cache`.
//!
//! These three stores together encode a release's device storage state
//! (Unmanaged / Pinned / CloudOnly; see [`crate::album_detail::storage_state`]).
//! An invalid combination — `managed = false` with neither an unmanaged source
//! nor a pinned cache, i.e. nowhere to read the audio (issue #105) — must be
//! unrepresentable. The guarantee is structural: the raw row/column writers are
//! private to this module, and the only `pub(crate)` mutators are the named,
//! atomic *transitions* below. Each transition is total over valid states and
//! commits whole, so no caller can leave a release in the invalid tuple.
//!
//! Callers (import, transfer, the upload observer, playback) reach these stores
//! ONLY through a transition or a getter — never a raw `INSERT`/`UPDATE`/`DELETE`
//! against the tables. A grep for the table names outside this module finds only
//! reads (the storage-state SELECTs in `client.rs`).

use chrono::{DateTime, Utc};
use coven::database::DbError;
use coven::rusqlite::{params, Connection, OptionalExtension, Row};
use tracing::warn;

use super::client::Database;
use super::models::{DbFileCacheEntry, DbUnmanagedSource};

// ── Private raw row writers (the ONLY code that mutates the three stores) ─────

/// Set the shared `releases.managed` fact, bumping `_updated_at` so the change
/// syncs.
fn set_managed_row(
    conn: &Connection,
    release_id: &str,
    managed: bool,
    reg: &str,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE releases SET managed = ?, _updated_at = ? WHERE id = ?",
        params![managed, reg, release_id],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Insert or replace the release's `release_unmanaged_source` row, preserving an
/// existing `delete_after_upload` intent (owned by the delete-intent setter).
fn upsert_unmanaged_source_row(
    conn: &Connection,
    release_id: &str,
    path: &str,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO release_unmanaged_source (release_id, path, delete_after_upload) \
         VALUES (?, ?, 0) \
         ON CONFLICT (release_id) DO UPDATE SET path = excluded.path",
        params![release_id, path],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Drop the release's `release_unmanaged_source` row. A missing row is a no-op.
fn delete_unmanaged_source_row(conn: &Connection, release_id: &str) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM release_unmanaged_source WHERE release_id = ?",
        params![release_id],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Insert or replace one `file_cache` row.
fn upsert_file_cache_row(
    conn: &Connection,
    file_id: &str,
    pinned: bool,
    size_bytes: i64,
    now: &str,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO file_cache (file_id, pinned, size_bytes, last_accessed_at) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (file_id) DO UPDATE SET \
             pinned = excluded.pinned, \
             size_bytes = excluded.size_bytes, \
             last_accessed_at = excluded.last_accessed_at",
        params![file_id, pinned, size_bytes, now],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Clear `pinned` on every `file_cache` row of a release's files (the rows
/// remain — now evictable cache entries).
fn unpin_release_cache_rows(conn: &Connection, release_id: &str) -> Result<(), DbError> {
    conn.execute(
        "UPDATE file_cache SET pinned = 0 \
         WHERE file_id IN (SELECT id FROM release_files WHERE release_id = ?)",
        params![release_id],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Delete every `file_cache` row of a release's files (the bytes are deleted
/// separately by the caller's deferred-deletion queue).
fn delete_release_cache_rows(conn: &Connection, release_id: &str) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM file_cache \
         WHERE file_id IN (SELECT id FROM release_files WHERE release_id = ?)",
        params![release_id],
    )
    .map(|_| ())
    .map_err(DbError::from)
}

/// Write an import's device storage rows inside the finalize transaction. The
/// import is one of three valid landings: an unmanaged source (unmanaged import,
/// or the cloud-only import that is a real unmanaged import which also queues
/// uploads — issue #105), a per-file pinned cache (the pin import), or neither
/// at finalize for the cloud-only case once it carries its source. Kept here so
/// the import's writes go through this module like every other transition.
pub(super) fn write_import_storage_rows(
    conn: &Connection,
    unmanaged_source: Option<&DbUnmanagedSource>,
    pinned_cache: &[DbFileCacheEntry],
    now: &str,
) -> Result<(), DbError> {
    if let Some(source) = unmanaged_source {
        upsert_unmanaged_source_row(conn, &source.release_id, &source.path)?;
    }
    for entry in pinned_cache {
        upsert_file_cache_row(conn, &entry.file_id, entry.pinned, entry.size_bytes, now)?;
    }
    Ok(())
}

// ── Public transitions (named for the lifecycle event; atomic; total over
//    valid states only) ─────────────────────────────────────────────────────

impl Database {
    /// Pin a release's files: insert a pinned `file_cache` row for each
    /// `(file_id, size_bytes)` (`pinned = true`, `last_accessed_at = now`) and
    /// drop any unmanaged source row — a fully pinned release is no longer held
    /// as an unmanaged source, so the two can't coexist (keeping the valid-state
    /// invariant by construction). `managed` is unchanged. Backs the import-pin
    /// (staged bytes), the Pin action (downloaded bytes, no source to drop), and
    /// the Manage-pin path (drops the source the import left). `release_id`
    /// names the release the files belong to.
    pub(crate) async fn set_pinned_cache(
        &self,
        release_id: &str,
        files: &[(String, i64)],
    ) -> Result<(), DbError> {
        let (release_id, files) = (release_id.to_string(), files.to_vec());
        let now = self.clock().now().to_rfc3339();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            for (file_id, size_bytes) in &files {
                upsert_file_cache_row(&tx, file_id, true, *size_bytes, &now)?;
            }
            delete_unmanaged_source_row(&tx, &release_id)?;
            tx.commit().map_err(DbError::from)
        })
        .await
    }

    /// Finish a managed transition that keeps this device's pinned cache: mark
    /// `managed = true`, leaving the `file_cache` rows in place. The upload
    /// observer's pinned branch.
    pub(crate) async fn mark_managed_pinned(&self, release_id: &str) -> Result<(), DbError> {
        self.mark_managed(release_id, false).await
    }

    /// Finish a managed transition to cloud-only: mark `managed = true` and drop
    /// this device's unmanaged source row (the originals are no longer the live
    /// copy). The upload observer's cloud-only branch — it deletes the originals
    /// first iff `delete_after_upload` was set.
    pub(crate) async fn mark_managed_cloud_only(&self, release_id: &str) -> Result<(), DbError> {
        self.mark_managed(release_id, true).await
    }

    /// Shared body of the two managed flips: set `managed = true` in one
    /// transaction, optionally dropping the unmanaged source row (cloud-only) or
    /// leaving it absent already (pinned). Kept private so the two observer
    /// branches stay distinct named transitions at the call site.
    async fn mark_managed(&self, release_id: &str, drop_source: bool) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let reg = self.register_stamp();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            set_managed_row(&tx, &release_id, true, &reg)?;
            if drop_source {
                delete_unmanaged_source_row(&tx, &release_id)?;
            }
            tx.commit().map_err(DbError::from)
        })
        .await
    }

    /// Unpin a release: clear `pinned` on its `file_cache` rows. The rows remain
    /// (now evictable cache entries); the bytes stay until eviction reclaims
    /// them.
    pub(crate) async fn unpin(&self, release_id: &str) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            unpin_release_cache_rows(&tx, &release_id)?;
            tx.commit().map_err(DbError::from)
        })
        .await
    }

    /// Unmanage a release: mark `managed = false`, record this device's
    /// unmanaged source at `path` (the files moved back out in place), and drop
    /// the release's `file_cache` rows.
    pub(crate) async fn unmanage(&self, release_id: &str, path: &str) -> Result<(), DbError> {
        let (release_id, path) = (release_id.to_string(), path.to_string());
        let reg = self.register_stamp();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            set_managed_row(&tx, &release_id, false, &reg)?;
            upsert_unmanaged_source_row(&tx, &release_id, &path)?;
            delete_release_cache_rows(&tx, &release_id)?;
            tx.commit().map_err(DbError::from)
        })
        .await
    }

    // ── Getters ──────────────────────────────────────────────────────────────

    /// This device's `release_unmanaged_source` row, if any. `None` means this
    /// device holds no unmanaged source for the release.
    pub async fn get_unmanaged_source(
        &self,
        release_id: &str,
    ) -> Result<Option<DbUnmanagedSource>, DbError> {
        let release_id = release_id.to_string();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT release_id, path, delete_after_upload \
                 FROM release_unmanaged_source WHERE release_id = ?",
                params![release_id],
                |row| {
                    Ok(DbUnmanagedSource {
                        release_id: row.get("release_id")?,
                        path: row.get("path")?,
                        delete_after_upload: row.get("delete_after_upload")?,
                    })
                },
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// This device's `file_cache` rows for a release's files. A row plus the
    /// file on disk is a cache hit; empty means this device caches none of the
    /// release's files.
    pub async fn get_release_file_cache(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbFileCacheEntry>, DbError> {
        let release_id = release_id.to_string();
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT fc.file_id, fc.pinned, fc.size_bytes, fc.last_accessed_at \
                 FROM file_cache fc \
                 JOIN release_files rf ON rf.id = fc.file_id \
                 WHERE rf.release_id = ?",
            )?;
            let rows = stmt.query_map(params![release_id], parse_cache_entry_row)?;
            rows.collect::<coven::rusqlite::Result<Vec<_>>>()
                .map_err(DbError::from)
        })
        .await
    }

    /// This device's `file_cache` row for a single file, if any.
    pub(crate) async fn get_file_cache_entry(
        &self,
        file_id: &str,
    ) -> Result<Option<DbFileCacheEntry>, DbError> {
        let file_id = file_id.to_string();
        self.with_conn(move |conn| {
            conn.query_row(
                "SELECT file_id, pinned, size_bytes, last_accessed_at \
                 FROM file_cache WHERE file_id = ?",
                params![file_id],
                parse_cache_entry_row,
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }

    /// Set the deferred-delete intent on the release's `release_unmanaged_source`
    /// row. The upload observer reads it on the last finished upload, deletes the
    /// originals, then drops the row. The row always exists when this is called
    /// (the intent is only set on an unmanaged release mid-Manage), so a missing
    /// row is a bug, not a no-op.
    pub async fn set_delete_after_upload(
        &self,
        release_id: &str,
        delete: bool,
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.with_conn(move |conn| {
            let affected = conn.execute(
                "UPDATE release_unmanaged_source SET delete_after_upload = ? WHERE release_id = ?",
                params![delete, release_id],
            )?;
            if affected == 0 {
                return Err(DbError(format!(
                    "release_unmanaged_source row missing for release {release_id}"
                )));
            }
            Ok(())
        })
        .await
    }

    /// Test-only: record an unmanaged source for a release at `path`, so it
    /// reads as the Unmanaged state. Mirrors what an unmanaged / cloud-only
    /// import lands; production code writes this through the import finalize
    /// transaction or the `unmanage` transition.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn test_set_unmanaged_source(
        &self,
        release_id: &str,
        path: &str,
    ) -> Result<(), DbError> {
        let (release_id, path) = (release_id.to_string(), path.to_string());
        self.with_conn(move |conn| upsert_unmanaged_source_row(conn, &release_id, &path))
            .await
    }

    /// Test-only: strand a release in the CloudOnly state — `managed = true`
    /// with the unmanaged source dropped (the observer's cloud-only flip).
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn test_mark_managed_cloud_only(&self, release_id: &str) -> Result<(), DbError> {
        self.mark_managed_cloud_only(release_id).await
    }

    /// Test-only: mark a release managed while keeping its pinned cache (the
    /// observer's pinned flip).
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn test_mark_managed_pinned(&self, release_id: &str) -> Result<(), DbError> {
        self.mark_managed_pinned(release_id).await
    }

    /// Test-only: pin every file of a release so it reads as the Pinned state.
    /// Inserts a pinned `file_cache` row (`size_bytes = 0`) for each of the
    /// release's `release_files`. The release must have at least one file for
    /// the Pinned derivation to hold.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn test_pin_all_files(&self, release_id: &str) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let now = self.clock().now().to_rfc3339();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let file_ids: Vec<String> = {
                let mut stmt = tx.prepare("SELECT id FROM release_files WHERE release_id = ?")?;
                let rows = stmt.query_map(params![release_id], |row| row.get::<_, String>(0))?;
                rows.collect::<coven::rusqlite::Result<Vec<_>>>()?
            };
            for file_id in &file_ids {
                upsert_file_cache_row(&tx, file_id, true, 0, &now)?;
            }
            tx.commit().map_err(DbError::from)
        })
        .await
    }
}

/// Parse an RFC3339 timestamp stored in `file_cache.last_accessed_at`. The
/// column is always written by [`upsert_file_cache_row`] as `to_rfc3339`, so a
/// malformed value is local-DB corruption — rare and abnormal. `last_accessed`
/// only orders eviction (a later PR), so a corrupt one isn't worth aborting a
/// read over: log it and treat it as the epoch (evict-first ordering, so a bad
/// row re-fetches from the cloud rather than lingering). The `warn!` keeps the
/// corruption diagnosable rather than silently masked.
fn parse_rfc3339(s: &str) -> DateTime<Utc> {
    match DateTime::parse_from_rfc3339(s) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => {
            warn!(value = %s, "corrupt file_cache.last_accessed_at, treating as epoch: {e}");
            DateTime::<Utc>::UNIX_EPOCH
        }
    }
}

/// Read a [`DbFileCacheEntry`] from a `file_cache` row. Shared by the
/// per-release and single-file getters so the column mapping lives once.
fn parse_cache_entry_row(row: &Row) -> coven::rusqlite::Result<DbFileCacheEntry> {
    let last: String = row.get("last_accessed_at")?;
    Ok(DbFileCacheEntry {
        file_id: row.get("file_id")?,
        pinned: row.get("pinned")?,
        size_bytes: row.get("size_bytes")?,
        last_accessed_at: parse_rfc3339(&last),
    })
}
