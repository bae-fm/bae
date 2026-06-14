# Sync

When a library has a cloud home configured, bae syncs metadata and files to it. Your own devices sync one library through the same cloud home. A device connects to the library with a restore code that carries the owner's signing key, so every device reads and writes as the one owner.

You bring your own storage (Google Drive, Dropbox, S3). bae layers encryption, signed changesets, and a membership chain on top. The cloud provider stores opaque blobs -- all trust lives in cryptography, not in the storage backend.

## Encryption

One symmetric master key per library. It encrypts the sync metadata (changesets, snapshots, heads, membership chain). A release's content — its audio files and its cover image — is encrypted with a random per-release key (`releases.content_key`) instead, independent of the master and of every other release. Artist images use the master key. Local files on the user's disk are always plaintext -- encryption is applied only when uploading to the cloud home. The master key is separate from the owner's Ed25519 signing key -- having it lets a device read/write data but not sign changesets as the owner.

**Key fingerprint:** SHA-256 of the key, truncated. Stored in `config.yaml`. Lets us detect the wrong key immediately instead of silently producing garbage.

When cloud is configured, bae generates an encryption key and stores it in the OS keyring. On macOS, this prompts for keychain access -- the user should understand bae is storing the encryption key in the system's secure store, not asking for a bae password.

## The CloudHome Trait

The `CloudHome` trait is the abstraction every cloud provider implements. Everything above this trait is shared across providers -- the sync protocol, encryption, membership chain, cloud home layout. Everything below it adapts to the specific cloud provider.

### What's shared across providers (above the trait)

- **Cloud home layout**: `changes/`, `heads/`, `snapshot.db.enc`, `images/`, `storage/` -- same logical paths regardless of backend
- **Encryption**: a per-library master key encrypts sync metadata; a release's content (audio + cover) uses a random per-release key — everything encrypted before it leaves the device
- **Sync protocol**: changesets, snapshots, conflict resolution via LWW -- same algorithm everywhere
- **Membership chain**: append-only log with Ed25519 signatures; the single owner entry, with the master key wrapped to the owner's pubkey

### What varies (below the trait)

- **Storage API**: how files are actually read/written (S3 API vs Google Drive API vs Dropbox API)
- **Authentication**: how a device authenticates to the cloud home (the user's own cloud account vs stored credentials)
- **Change notifications**: consumer clouds have push notifications, S3 requires polling

### The trait

```rust
trait CloudHome {
    // storage -- same interface, different API underneath
    fn write(path, data) -> Result
    fn read(path) -> Result<Bytes>
    fn read_range(path, start, end) -> Result<Bytes>
    fn list(prefix) -> Result<Vec<String>>
    fn delete(path) -> Result
    fn exists(path) -> Result<bool>
}
```

Implementations: S3 (via aws-sdk-s3), Google Drive, Dropbox, OneDrive (via their REST APIs), iCloud Drive (via local filesystem).

### Storage operations

Every cloud storage service supports basic file operations -- the trait normalizes them into one interface:

| Operation | S3 | Google Drive | Dropbox |
|---|---|---|---|
| Write | `PutObject` | `files.create` | `files/upload` |
| Read | `GetObject` | `files.get` (media) | `files/download` |
| Read byte range | `Range` header | `Range` header | `Range` header |
| List by prefix | `ListObjectsV2(prefix=)` | `files.list(q=)` on folder | `files/list_folder` |
| Delete | `DeleteObject` | `files.delete` | `files/delete` |
| Exists | `HeadObject` | `files.get(fields=id)` | `files/get_metadata` |

Cloud home paths like `changes/{device_id}/{seq}.enc` map to flat object keys on S3 and folder hierarchy on consumer clouds. Same logical paths, different native representations. Callers (sync engine, image uploader, file storage) don't know or care which backend they're talking to.

### How a device reaches the cloud home

A restore code carries the cloud home coordinates and the owner's signing key. How the connecting device then authenticates to the storage backend depends on the backend:

| | Consumer cloud | S3 |
|---|---|---|
| **Coordinates** | Provider type + folder ID | Bucket + region + endpoint |
| **Authenticates with** | The user signs into their own cloud account (OAuth) | Stored S3 credentials |

### Change notifications

Consumer clouds support push-based change notifications (Google Drive `changes.watch`, Dropbox longpoll, OneDrive delta API) which enable faster sync than S3's polling model. The `CloudHome` trait can optionally support a `watch` method for backends that have it.

## Changeset sync

A library can be used on multiple devices. Each device has a `device_id` (UUID, auto-generated on first startup, stored in `config.yaml`), its own local copy of `library.db`, and syncs changes through the cloud home. The sync mechanism uses the SQLite session extension to capture exactly what changed, push the changeset to the cloud home, and pull and apply on other devices with a conflict handler. No coordination server.

Each device writes to its own keyspace in the cloud home. No write contention by construction:

```
cloud-home/
  snapshot.db.enc                  # full DB for bootstrapping new devices
  changes/{device_id}/{seq}.enc    # changeset blobs per device
  heads/{device_id}.json.enc       # "my latest seq is 42"
  images/ab/cd/{id}                # all library images (encrypted)
  storage/ab/cd/{file_id}          # release files (encrypted)
```

**Push**: after a logical operation (import a release, delete a release, edit metadata), end the session to get the changeset, encrypt it, upload to `changes/{your_device}/{seq}.enc`, and update `heads/{your_device}.json.enc` with the new sequence number.

**Pull**: list `heads/`, compare each device's sequence number to your local cursors. If anyone's ahead, fetch their new changesets, decrypt, and apply with the conflict handler. Deterministic -- same changesets in the same order produce the same result.

**Polling is cheap** -- listing `heads/` is one S3 LIST call. If all seqs match your cursors, nothing to do. Check on app open + periodic timer.

> **Why the session extension?**
>
> We considered and rejected several alternatives:
>
> - **Op log with method wrapping** -- requires intercepting every write method, maintaining a custom JSON op format, and building a custom merge algorithm. An import that creates an album + release + 12 tracks + 12 files would generate ~50 operations. The session extension produces a single compact changeset containing only the rows and columns that actually changed.
> - **SQLite triggers** -- same maintenance burden moved to SQL; must enumerate every column of every synced table
> - **CRDTs (Automerge/Loro)** -- hold full state in memory, don't scale to arbitrary entity counts
> - **cr-sqlite** -- stalled project, too risky as a dependency
>
> The session extension is built into SQLite. It tracks all changes (INSERT/UPDATE/DELETE) automatically at the C level. No triggers, no method wrapping, no column enumeration. The app writes normally. SQLite records what changed. We grab the changeset and push it.

### Conflict resolution: row-level LWW

When two devices change the same row, the later `_updated_at` timestamp wins. Every synced table has an `_updated_at` column maintained by a Hybrid Logical Clock (HLC).

The session extension's `sqlite3changeset_apply()` calls a conflict handler for each conflicting operation. The handler compares `_updated_at` and returns REPLACE (accept incoming) or OMIT (keep local).

**Crucially, non-conflicting edits to different columns on the same row both survive.** A changeset for an UPDATE contains only the columns that changed. When we REPLACE, only those columns are overwritten -- the rest keep their local values.

**Example -- no conflict (different columns):**
```
Device A: edits title on rel-123 at T1
Device B: edits year  on rel-123 at T2
Result: both title and year changes survive
```

**Example -- conflict (same column):**
```
Device A: edits title on rel-123 at T1
Device B: edits title on rel-123 at T2
Result: Device B's title wins (later _updated_at)
```

**Example -- delete vs. edit:**
```
Device B: edits genre on rel-123 at T1
Device A: deletes rel-123 at T2
Result: rel-123 is deleted (delete wins)
```

For a music library this is fine -- conflicts are rare and low-stakes. Worst case someone re-edits a field.

### The sync protocol

A **session** is the session extension's recording context — you attach it to specific tables and it silently records all changes (INSERT/UPDATE/DELETE) made through that connection. When you're ready to sync, you extract the accumulated changeset and end the session.

```
1. Start session
2. App writes normally...
3. Time to sync:
   a. Grab changeset from session
   b. End session
   c. Push changeset to cloud home
   d. Pull incoming changesets (NO session active)
   e. Apply incoming with conflict handler
   f. Start new session
```

### Sync triggers

Sync pushes after `LibraryEvent::AlbumsChanged` (import, delete, edit) with debounce. If the cloud home is unreachable, sync is skipped and retried next time.

### Database architecture

The session extension attaches to a single connection and only captures changes made through that connection. The database uses a dedicated write connection (with session attached) and a read pool. This matches SQLite's single-writer-multiple-reader architecture.

### Schema evolution

The SQLite session extension identifies columns by index, not by name. This constrains how the schema can change once changeset sync is live.

**Additive changes are transparent.** Adding columns at the end of a table or adding new tables requires no coordination. Old changesets applied to a new schema just have fewer columns (extras keep defaults). New changesets applied to an old schema skip unknown columns. Devices on different schema versions interoperate seamlessly.

**Breaking changes require coordination.** Deleting, reordering, or renaming columns shifts column indices and corrupts changeset application. These changes bump a `min_schema_version` marker in the cloud home, splitting the changeset history into **epochs**. Within an epoch, all changesets are schema-compatible. Across epochs, no replay -- devices pull a fresh snapshot to jump forward. This means any schema change is possible (the snapshot IS the migrated state), but all devices must upgrade before syncing resumes.

Every changeset envelope carries a `schema_version` integer so receivers know what schema produced it. In practice, schema changes for a music library are almost always additive (new fields), making breaking migrations rare.

### Snapshots

The changeset log grows forever without intervention. Periodically, any device writes a snapshot -- a full DB `VACUUM INTO`. The snapshot includes its cursors (per-device sequence numbers at the time it was written), so a new device knows exactly where to start pulling changesets from.

```
snapshot.db.enc   # overwritten each time, includes cursors
```

New devices start from the snapshot, then replay only changesets after it. Garbage collection is separate from snapshots -- a changeset can only be deleted when every device's cursor (from `heads/`) has moved past it, meaning all devices have already applied it.

## One owner, many devices

A library has one owner identity. Putting it on a second device -- so two machines read and write the same library -- still needs identity, authorization, and signature validation, even though there is only one person.

### Identity = the owner keypair

Your identity is one keypair (Ed25519 for signing, X25519 for encryption), generated locally on first use and held in the OS keyring. The public key is the owner identity. A connecting device receives this signing key through the restore code, so every device signs changesets as the one owner.

### Bucket layout

```
cloud-home/
  membership/{pubkey}/{seq}.enc     # signed membership entries (per author, avoids S3 overwrite races)
  keys/{owner_pubkey}.enc           # library key wrapped to the owner's public key
  heads/{device_id}.json.enc        # per-device head pointer
  changes/{device_id}/{seq}.enc     # per-device changeset stream (signed)
  snapshot.db.enc
  images/ab/cd/{id}
  storage/ab/cd/{file_id}
```

Changesets are keyed by `device_id` (the owner has multiple devices). Authorship is established cryptographically: each changeset envelope includes `author_pubkey` and a signature over the changeset bytes.

### Membership chain

An append-only log, stored as individual files to avoid S3 overwrite races. For a bae library it holds a single self-signed owner entry -- the authoritative record of the owner public key whose signing key authorizes every device.

```json
{ "action": "add", "user_pubkey": "...", "role": "owner",
  "ts": "2026-01-01T...", "author_pubkey": "...", "sig": "..." }
```

On read, clients download the membership entry and validate it. Changeset signatures are verified against the owner public key from this chain.

### Connecting a device

The owner generates a **restore code** on a device that already has the library. The code carries the cloud home coordinates and the owner's signing key. On another device, pasting the code:

1. Reads the cloud home coordinates and authenticates to the storage backend (the user's own cloud account on consumer clouds, stored credentials on S3).
2. Unwraps the master key from `keys/{owner_pubkey}.enc`.
3. Downloads and validates the membership entry.
4. Downloads the snapshot, pulls changesets, applies them -> has the full library.
5. Pushes its own signed changesets from then on, as the owner.

### Changeset validation on pull

Before applying any changeset:
1. Verify the signature against `author_pubkey`
2. Is the author the library owner?
3. If either fails -> discard

## How It Composes

**Local only**:
- No cloud home configured. All files stay on disk, bae indexes in place.
- No sync, no cloud. All features work locally.

**Multiple devices**:
- Changeset sync between your devices via the shared cloud home
- Same owner identity, different device IDs, merge via LWW

