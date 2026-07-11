# Bump coven to the write-aware empty-capture warning (+ library→store rename)

## Why (the true story)

An earlier plan adopted `CovenHandle::sql_local` to silence coven's
once-per-second empty-capture warning during playback. That was **reverted
upstream**. coven main (2ea939e) instead made the warning *write-aware*: a
journaled `sql()`/`call_sql()` transaction warns only when it changed **zero
rows** (a `total_changes` delta), not when its synced capture was empty. So a
device-local write (playback_state once a second, cloud_outbox mutations) is
silent on the plain journaled path because it changes rows — **no `sql_local`,
no `local` entry point, no db-client changes at all.** `call` stays exactly as
merged.

## 1. Bump the pins

`Cargo.toml`: `coven` and `coven-core` → `2ea939ec6711…` (same rev);
`cargo update -p coven -p coven-core`.

The crossing also includes **e4ac5ee "Rename the domain concept: library →
store"** — a coven public-API rename. Adapt bae's *consumption* of the renamed
API to the new names, no compat aliases:

- `coven::LibraryDir` → `coven::StoreDir` (type; bae never defines its own).
- Fields bae reads on `coven::Config` (directly or through bae's `Config`
  `Deref`): `library_id/library_name/library_dir` → `store_id/store_name/store_dir`.
  This includes bae's `coven::Config { … }` construction keys.
- `KeyService::library_id()` → `store_id()`.
- `coven` `InviteCodeInfo`/`RestoreCodeInfo` `.library_id/.library_name` →
  `.store_id/.store_name`.

**Scope line:** this renames only where bae touches coven's renamed public API.
bae's own product concept — a music **library** — is a different thing from
coven's synced **store**; bae's `ConfigYaml` keys, `BridgeLibrary`,
`restore_from_cloud`/`rename_library` params, `LibraryManager`, and UI keep
"library". Renaming those would cascade across Rust/Swift/Kotlin/UI/on-disk
format as a separate product decision, out of scope here. No aliases either way.

## 2. Tripwire test for the new semantics

`test_sql_read_tripwire.rs`: the warning text is now "…changed nothing" and it
fires on zero rows changed.

- Positive control was `add_cloud_outbox_delete`, which now **changes rows** and
  stays silent. Replace it with a deliberate **zero-change closure through the
  raw `handle.sql()`** (a `SELECT`), which changes no rows and must warn.
- Keep the 13 read negative assertions.
- **Add** a negative assertion that `add_cloud_outbox_delete` (it changes rows)
  does not warn — the case that motivated the whole change.

## 3. Verify

`scripts/check.sh` green (exit 0). Build/test with
`CARGO_TARGET_DIR=target-iso RUSTC_WRAPPER=`; never `--no-verify`. Commit(s) on
`adopt-sql-local`, include this plan; do not push.
