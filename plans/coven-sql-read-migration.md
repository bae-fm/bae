# Coven bump + journal-free reads in the db client

## Situation

bae pins coven at `83e3321a`, ~21 commits behind coven main
(`bf5ced59f4247cd96a2a2820007205222b07d274`). Coven main adds
`CovenHandle::sql_read`: pure reads run on a separate `SQLITE_OPEN_READONLY`
WAL connection — no changeset session, no journal, concurrent with the writer
instead of queued behind it — with the closure shape
`FnOnce(&rusqlite::Connection) -> CovenResult<R>`. Coven also elevated its
empty-changeset journal log to `warn!` as a tripwire: once reads are
migrated, that warning firing at steady state means a read is still on the
journaled write path.

All of bae's SQL runs through the `Database` client in
`bae-core/src/db/client/mod.rs`: `call` (closure over `&Connection` via
`sql.tx()`) and `call_sql` (closure over `SqlContext`, for writes that stamp),
both funneling into `CovenHandle::sql` — so today every bae read pays the
journal.

## Changes

### 1. Bump the coven pin

`Cargo.toml`: coven `rev = "bf5ced59f4247cd96a2a2820007205222b07d274"`;
`cargo update -p coven`.

The crossing includes: per-uploader blob keyspace + local uploader index
(ee815af, c5d1dc0, 5e1f4ff), blob GC own-prefix sweep (ea79fa1), public-API
curation to crate-root re-exports (4c9d96a), Secret Debug redaction (f027f23),
sync-status outcome enum + reconnect (7074f21), sync-cycle result detail
(3b6aa5d), PullResult.row_changes as refresh hint (1424735), RFC 3339
last_sync (abf57e1), member-removal adoption error (d790acc), Merge split out
of Sync (c0de754), read-only open (04d2a08), sql_read (bf5ced5).

Adapt bae to whatever breaks, adopting the new shapes directly. **Per repo
policy (AGENTS.md): pre-1.0, `rm -rf ~/.bae` is the migration strategy — no
compat shims, no dual-shape handling, no data migration for the blob-layout
changes.** If a crossing commit changed a contract bae consumes (sync status,
pull results, error variants), rewrite bae's consumer to the new contract.

### 2. `read` on the db client

In `bae-core/src/db/client/mod.rs`, alongside `call`/`call_sql`:

```rust
/// Run a pure read on coven's read-only companion connection: no changeset
/// journal, concurrent with the writer. The closure cannot write (the
/// connection is SQLITE_OPEN_READONLY — SQLite refuses DML).
async fn read<R>(
    &self,
    f: impl FnOnce(&Connection) -> Result<R, DbError> + Send + 'static,
) -> Result<R, DbError>
where
    R: Send + 'static,
{
    self.inner
        .handle
        .sql_read(move |conn| f(conn).map_err(CovenError::from))
        .await
        .map_err(Self::coven_error)
}
```

Document the resulting contract on all three: `read` = pure reads;
`call` = writes that don't stamp; `call_sql` = writes that stamp
`_updated_at`.

### 3. Migrate the read call sites

Go through every `call(` site in `bae-core/src/db/client/*.rs` (album,
artist, blobs, identity, playback, release, track, mod) and classify:

- Closure only queries (SELECT / query_row / prepare+query) → move to `read`.
- Closure executes INSERT/UPDATE/DELETE → stays on `call`.
- `call_sql` sites stay (stamping writes by construction).

Read-your-writes holds for committed writes (the WAL reader sees the last
committed state), and every `call`/`call_sql` write commits before its future
resolves — so a read that follows an awaited write stays correct on the read
connection. A mixed closure (read then conditional write) counts as a write.

Report the classification table: per file, how many sites moved to `read`,
how many stayed.

### 4. Tripwire check

Run the bae-core test suite with the coven warning visible and capture it:

```
RUST_LOG=warn cargo test -p bae-core 2>&1 | tee /tmp/bae-test-log
grep "produced no synced changes" /tmp/bae-test-log
```

Steady-state read paths must not fire it. Hits from tests that intentionally
no-op conditional writes are legitimate — list any hits with a one-line
classification rather than chasing zero blindly.

## Verification

- `scripts/check.sh` green (full local CI: workspace tests, clippy, platform
  builds, lints). Build/test with `CARGO_TARGET_DIR=target-iso RUSTC_WRAPPER=`
  (one warm dir, sccache off), inline on `git commit` too so the pre-commit
  hook reuses the dir. Never `--no-verify`.
- Tests needing FFmpeg dylibs: `DYLD_LIBRARY_PATH=$PWD/bae-ffmpeg/dist/lib`.

## Commits

Two commits on this branch if the bump adaptation is nonempty: (1) the pin
bump + API adaptation, (2) the read migration. One commit if the bump adapts
cleanly with no source changes. Include this plan file. Messages: why, not
what.

## Out of scope

- No coven changes.
- No behavior changes to writes.
- No UI work; bridge/platform edits only where the coven bump forces them.
