# Coven bump: 2ea939e..12a09cb (KeyService split, StoreLayout, RestoreSource collapse)

Bump both `coven` and `coven-core` pins to `12a09cb8aad337fee61805044b02a60bca79c1da`
and adapt bae to the crossing. Pre-1.0; `rm -rf ~/.bae` is the migration policy.

## Commit-by-commit bae surface

- **9814308 Split KeyService into DeviceKeys and StoreKeys** — dominant.
  `coven::KeyService` is gone. The device Ed25519 signing identity is
  `DeviceKeys` (stateless associated fns, no store_id); a store's encryption
  key / cloud creds / OAuth tokens are `StoreKeys` (store-scoped). bae uses
  `KeyService` pervasively; almost every site is store-scoped → `StoreKeys`.
  Only two sites call the signing key (`get_or_create_user_keypair`) →
  `DeviceKeys`: `library/mod.rs` `restore_from_cloud`, `library/manager/tests.rs`.
  bae re-exports and extends `KeyService` in `bae-core/src/keys.rs`
  (`BaeKeyServiceExt`: Discogs key, MCP token, forget-encryption — all
  store-scoped) → re-export `StoreKeys`+`DeviceKeys`, trait becomes
  `BaeStoreKeysExt` impl for `StoreKeys`.
- **ad794f9 StoreLayout** — join/restore take `&StoreLayout` instead of an
  app_dir path; the old code hardwired `<app_dir>/stores/<id>/store.db`. bae
  keeps libraries under `<bae_dir>/libraries/<id>/store.db`, so the old join/
  restore wrote to the wrong `stores/` dir (a latent inconsistency). Pass
  `StoreLayout::new(bae_dir).stores_dirname("libraries")` so join/restore land
  in the same place create + discovery use.
- **950689f RestoreProvider→CloudHomeJoinInfo; 4f82659 invite hardening** —
  `RestoreSource` collapses from an enum (S3/CloudKit/GoogleDrive/Dropbox/
  OneDrive variants) to a struct `{ join_info: CloudHomeJoinInfo, oauth_tokens,
  cloudkit_ops }`. `bae-bridge/src/setup.rs::restore_from_cloud` builds the new
  struct from `BridgeRestoreSource` (S3 gets `key_prefix: None`, preserving prior
  bae behavior). Decode DTO fields (`store_id`, `cloud_provider`, `needs_oauth`)
  unchanged → bridge preview types untouched.
- **c50916b one BootstrapError** — join/restore now return
  `coven::BootstrapError` (was JoinError/RestoreError, which bae never named).
  bae maps `BootstrapError::Cancelled` → its own Cancelled, else `to_string()`.
- **23edeeb join/restore cancellation** — the four entry fns take a required
  `cancel: &watch::Receiver<bool>`. coven now owns cooperative cancellation AND
  residue cleanup on cancel (removes the store dir it created, exactly like a
  failure; 8683dbe guarantees it only ever removes a dir this invocation made).
  bae's `library/mod.rs` currently races the op with `tokio::select!` on a
  `CancellationToken` and does its own dir cleanup. Adopt coven's mechanism:
  bridge the `CancellationToken` to a `watch::Receiver<bool>`, pass it down, and
  delete bae's parallel select-race + `remove_cancelled_library_dir` +
  `CodeOperationCancel` machinery. Non-cancellable wrappers pass a never-firing
  receiver (channel(false) with the sender dropped).
- **cb3b76b SQL surface** — renamed `CovenReadHandle::sql`→`sql_read`. bae uses
  `CovenHandle::sql`/`sql_read` (the write handle, unchanged) and no
  `CovenReadHandle`. No bae change. Tripwire test uses the write handle; it only
  breaks through `Database::new_test`'s `KeyService`→`StoreKeys`.
- **b674dc1 Config.load / fc3941d / 539e5b7 / 4e38776 / 67eeeff / 12a09cb** —
  coven-internal or additive; no bae code change (12a09cb is the motivating pin).

## Edits

1. `Cargo.toml`: both pins → `12a09cb…`; `cargo update -p coven -p coven-core`. (done)
2. `bae-core/src/keys.rs`: re-export `StoreKeys`+`DeviceKeys`; `BaeStoreKeysExt`
   for `StoreKeys`.
3. Global `KeyService`→`StoreKeys` across bae (all crates, excluding keys.rs);
   `BaeKeyServiceExt`→`BaeStoreKeysExt` follows from the same rename.
4. Fix the two signing sites → `DeviceKeys::get_or_create_user_keypair()`.
5. `library/mod.rs`: `library_layout` helper; thread `StoreLayout` + cancel
   receiver; collapse the cancellation harness onto coven's; map `BootstrapError`.
6. `bae-bridge/src/setup.rs`: build the `RestoreSource` struct.
7. `db/client/mod.rs`: `key_service: coven::StoreKeys`.

## Verify

Build + test with `CARGO_TARGET_DIR=target-iso RUSTC_WRAPPER=` (FFmpeg dylibs via
`DYLD_LIBRARY_PATH=$PWD/bae-ffmpeg/dist/lib`). `scripts/check.sh` end to end,
exit 0. Commit on branch, plan included, not pushed.
