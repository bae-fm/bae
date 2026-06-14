# Storage Model

## Library types

A library is either **local** or **synced**. This is a library-level property.

**Local library** (no cloud home):
- Files stay wherever the user has them on disk. bae indexes them but does not copy or move them.
- Playback works locally. No multi-device sync, no offline/online distinction (everything is local).
- All imports are unmanaged (no storage mode choice).

**Synced library** (has a cloud home):
- Sync across your own devices works through the shared cloud home.
- The user chooses **storage mode per release** at import time (see below).

## Storage mode is a per-release choice

At import confirmation, the user picks one of:

- **Unmanaged**: bae indexes files in-place. No copying, no cloud involvement. `unmanaged_path` points to the source folder. This is the only option when the library has no cloud home.
- **Managed (pinned)**: files copied to `storage/`, uploaded to cloud, local copy kept. `pinned_locally = true`.
- **Managed (unpinned)**: files uploaded to cloud directly from source, no local copy in `storage/`. `pinned_locally = false`.

This means connecting a cloud home to an existing library does **not** automatically transition existing releases. Existing unmanaged releases stay unmanaged. Only new imports get the choice.

## File location resolution

Three cases based on release state:

1. **Unmanaged**: file path = `unmanaged_path` + `original_filename`
2. **Managed, pinned**: file at `~/.bae/libraries/{uuid}/storage/{ab}/{cd}/{file_id}` (plaintext). Also in cloud at `storage/{ab}/{cd}/{file_id}` (encrypted).
3. **Managed, unpinned (cloud only)**: file in cloud home at `storage/{ab}/{cd}/{file_id}`.

Playback picks the reader accordingly: `LocalFileReader` for unmanaged and pinned managed releases, `CloudReader` for unpinned managed releases.

## Pin for offline

Any managed release can be pinned or unpinned:

- **Pin**: download from cloud home, decrypt, write to local storage. Set `pinned_locally = true`.
- **Unpin**: delete local copy. Set `pinned_locally = false`. Files remain in cloud.

Pinning is not applicable to unmanaged releases (they're already local).

## What reaches another device

Only managed releases reach your other devices — their files live in the cloud home. Unmanaged releases never leave the disk of the device that imported them, so another device can't see them.

## Schema

On the `releases` table:

- `unmanaged_path TEXT` -- filesystem path for unmanaged releases (NULL for managed)
- `pinned_locally BOOLEAN NOT NULL DEFAULT FALSE` -- whether a local copy is kept for offline playback (only meaningful for managed releases)
- `content_key BLOB NOT NULL` -- the release's random 32-byte content encryption key, used for both its audio files and its cover image. A column on the synced `releases` row, so it reaches every one of your devices.

## Encryption

- **Local files are always plaintext.** No encryption on disk, ever.
- **Cloud content is encrypted per release.** Each release has its own random 32-byte key (`releases.content_key`, minted at release creation). Its audio files **and its cover image** are encrypted with that key before upload and decrypted with it on download — by another device or by playback. The key is independent of the master key and of every other release's key.
- **Artist images** use the master key.
- **The master key** encrypts the sync metadata — changesets, snapshots, heads, the membership chain. It is wrapped to the owner's identity key, never shared in the clear.
- Encryption/decryption happens at the boundary: upload encrypts, download decrypts.

## Cloud outbox

The cloud outbox (`cloud_outbox` table) tracks pending cloud storage operations for audio files. It decouples import/deletion from cloud availability.

### Uploads

Managed imports queue `upload` entries in the outbox. Each entry carries the release's `content_key` (copied from the release row at enqueue time, since the async upload runs long after the enqueue site is gone). The sync loop processes uploads before pushing the DB changeset:

1. For each pending upload: read file (from `source_path` if set, else from `storage/`), encrypt with the entry's `content_key`, `cloud_home.write()`
2. On success: remove outbox entry
3. On failure: log warning, skip (retry next cycle)
4. **Changeset push is deferred while upload entries remain** — remote devices never see releases whose audio files aren't in cloud yet

For **pinned** imports: `source_path` is NULL, upload reads from `storage/`. Import succeeds immediately (local write), cloud upload is async.

For **unpinned** imports: `source_path` points to the original file location. Upload reads directly from the source.

### Deletes

When a managed release is deleted, `delete` entries are added to the outbox tagged with the current local sync seq. Cloud files are deleted conservatively:

1. Changeset with the deletion is pushed first (remote devices learn about the deletion)
2. Delete entries are only processed when all known device heads have advanced past the deletion's seq
3. This ensures other devices can keep playing the release from cloud until they sync the deletion

If a device is offline for an extended period, cloud files for deleted releases are retained until that device catches up (or a configurable retention timeout).

### Ordering within a sync cycle

1. Process outbox `upload` entries (files must be in cloud before changeset references them)
2. If uploads remain: skip changeset push, continue to step 4
3. Push DB changeset
4. Pull remote changes
5. Process outbox `delete` entries (only those safe to delete based on device heads)
6. Snapshot if needed

## Offline and degraded cloud behavior

Import always succeeds, even when the cloud home is unreachable or full. The cloud outbox queues uploads for later. The DB changeset push is deferred until uploads complete, so the cloud never has metadata that references missing audio files.

Playback and all local features work regardless of cloud availability. Your other devices won't see new content until the cloud catches up — same as being offline.

## Adding a cloud home to a local library

When a user configures a cloud home on an existing local library, existing releases are **not** automatically transitioned. They stay unmanaged. New imports going forward get the managed/unmanaged choice in the import confirmation UI.
