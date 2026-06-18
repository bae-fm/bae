# Data Model: Releases, Files, and Images

## Libraries and storage

A **library** is the logical entity -- a music collection. It has an identity (`library_id`), a name, and an encryption key. It lives primarily at the **library home** (`~/.bae/libraries/{uuid}/`), where desktop writes the authoritative DB.

A library can optionally have a **cloud home** -- a single cloud location (Google Drive folder, S3 bucket, etc.) that serves as the hub for syncing the library across your own devices. The cloud home mirrors the library's data (encrypted) and adds the machinery for incremental sync. There is exactly one cloud home per library. It is configured in `config.yaml`.

A library is either **local** (no cloud home) or **synced** (has a cloud home). Storage mode is a **per-release** choice made at import time. See `notes/15-storage-model.md` for the full storage model.

- **Unmanaged release**: files stay wherever the user has them on disk. bae indexes them but does not copy or move them. The `unmanaged_path` on the release points to the source folder. This is the only option for local libraries, and also available in synced libraries.
- **Managed release (pinned)**: files copied to local `storage/`, uploaded to cloud, local copy kept. `pinned_locally = true`.
- **Managed release (unpinned)**: files uploaded to cloud directly from source, no local copy. `pinned_locally = false`. Streams from cloud on demand.

```
Library "My Music" (lib-111)
  ├── local home: ~/.bae/libraries/lib-111/    <- desktop writes here
  └── cloud home: Google Drive / S3 bucket     <- sync + images + release files
```

Release files are separate from metadata. Storage behavior is per-release, chosen at import time.

## Directory layout

### bae directory (`~/.bae/`)

Used by desktop -- this is what opens when you launch the app. Contains all local libraries. `active-library` is the UUID of the currently active library -- absent means use the first (or only) library.

```
~/.bae/
  active-library               # UUID of the active library
  libraries/
    {uuid}/                    # one directory per library
```

### Library home

The library home is where desktop runs. It holds the authoritative DB, device-specific config, and local release files.

```
~/.bae/libraries/{uuid}/
  config.yaml                  # device-specific settings (not synced)
  library.db                   # SQLite -- all metadata
  images/ab/cd/{id}            # library images (covers, artist photos -- no extension, content type in DB)
  storage/ab/cd/{file_id}      # release files (no extension, content type in DB)
  pending_deletions.json       # deferred file deletion manifest
```

**`config.yaml`** -- device-specific settings. Not synced, only at the library home. Includes things like cloud home configuration, server settings, keyring hint flags, and more. Non-secret only -- credentials go in the keyring.

### Keyring (OS keyring, service "bae", accounts namespaced by library_id)

Managed by coven's `KeyService`. Every entry lives under the keyring service name "bae" (the host app identity bae sets once at startup via `coven::keys::set_keyring_service`), so other coven-based apps on the same machine never collide. On macOS, uses the protected data store with iCloud Keychain sync. coven's `KeyService` reads and writes the keyring only — it does not read the environment. In dev mode bae bridges its `BAE_*` env vars into these accounts at startup (`config::seed_dev_keyring`).

- `encryption_master_key` -- one per library, used for all file and metadata encryption
- `cloud_home_credentials` -- serialized enum: S3 access+secret, OAuth token, or none (iCloud)
- `discogs_api_key` -- bae-domain credential layered on `KeyService` via `BaeKeyServiceExt`
- `server_password`
- `coven_user_signing_key` -- global (not library-scoped), Ed25519 signing key; the public key is derived from it on load, not stored separately

### Cloud home layout

The cloud home is one location per library (a Google Drive folder, S3 bucket, Dropbox folder, etc.). It mirrors the library's data and adds sync + access control machinery.

| Library home | Cloud home | Purpose |
|---|---|---|
| `library.db` | `snapshot.db.enc` | Full DB (bootstrap for new devices) |
| — | `changes/{device_id}/{seq}.enc` | Incremental sync changesets |
| — | `heads/{device_id}.json.enc` | Per-device sequence numbers (cheap polling) |
| `images/` | `images/ab/cd/{id}` | Library images (encrypted in cloud) |
| `storage/` | `storage/ab/cd/{file_id}` | Release files (encrypted in cloud) |
| — | `membership/{pubkey}/{seq}.enc` | Membership chain: the single owner entry |
| — | `keys/{owner_pubkey}.enc` | Library key wrapped to the owner's identity |
| `config.yaml` | — | Device-specific, not synced |
| `pending_deletions.json` | — | Device-specific, not synced |

Images in the cloud home are encrypted. Release files use chunked encryption.

Release files live under `storage/` in an opaque hash-based layout. `prefix` = first 2 chars of the file ID, `subprefix` = next 2 chars. No filenames, no extensions -- original filenames and content types live in the DB. The path is deterministic from the file ID alone: `storage/{prefix}/{subprefix}/{file_id}`. Same layout in both the library home and cloud home.

## Two classes of files

bae manages two fundamentally different kinds of files:

### Release files

Whatever files came with a release (audio, images, CUE sheets, logs, etc.). These are the user's data. bae stores them exactly as imported, preserving the original files for fidelity and export.

Where they live depends on the release's storage mode:
- **Unmanaged**: files stay at their original location on disk. `unmanaged_path` on the release points to the folder. bae indexes but doesn't touch the files.
- **Managed**: bae encrypts and uploads to `cloud-home/storage/ab/cd/{file_id}`. If pinned, a plaintext copy is also kept at `library_dir/storage/ab/cd/{file_id}`.

Local files (`library_dir/storage/`) are always stored as plaintext. Encryption only happens when files are uploaded to the cloud home.

Storage is tracked on the `releases` table with two columns: `unmanaged_path` (filesystem path for unmanaged releases, NULL for managed) and `pinned_locally` (whether a local copy is kept for offline playback in managed releases). Managed file paths are derived from the file ID: `storage/{prefix}/{subprefix}/{file_id}`.

### Metadata images

Images that bae creates and manages. These live in the library home directory, not with the release files. They are synced to the cloud home as part of changeset sync (pushed when the `library_images` table changes).

Two kinds:
- **Release covers** -- display art for album grids, detail views, playback. One per release. May originate from a file in the release, or fetched from MusicBrainz/Discogs. bae makes its own copy.
- **Artist images** -- fetched from external sources.

All library images are stored under `images/` using the same hash-based prefixing as release files: `images/{prefix}/{subprefix}/{id}`. No extension on disk -- content type is in the DB.

## DB tables

### `release_files` -- release files (audio + images + metadata)

Tracks every file in a release. These travel with the release.

```
release_files
  id                TEXT PK
  release_id        TEXT FK -> releases
  original_filename TEXT NOT NULL    -- "01 - Track.flac", "cover.jpg", "disc.cue"
  file_size         INTEGER NOT NULL
  content_type      TEXT NOT NULL    -- "audio/flac", "image/jpeg", "text/plain"
  encryption_nonce  BLOB
  encryption_scheme TEXT             -- encryption algorithm identifier
  created_at        TEXT NOT NULL
  _updated_at       TEXT NOT NULL    -- sync metadata
```

Content types are stored as MIME strings and mapped to the `ContentType` enum in Rust for type-safe comparisons (`ContentType::Flac`, `ContentType::Jpeg`, etc.) with helpers like `is_audio()`, `is_image()`, `display_name()`, and `from_extension()`.

### `audio_formats` -- playback metadata (1:1 with tracks)

Stores everything needed to play a track: codec info, FLAC headers for CUE/FLAC tracks (where the decoder needs headers prepended since playback starts mid-file), byte offsets for track boundaries, and a dense seektable for frame-accurate seeking (~93ms precision).

For one-file-per-track FLAC: `start_byte_offset`/`end_byte_offset` are NULL, `needs_headers` is false. For CUE/FLAC: both offsets point into the shared FLAC file, `needs_headers` is true.

`file_id` links to the `release_files` row containing this track's audio data.

```
audio_formats
  id                  TEXT PK
  track_id            TEXT FK -> tracks (UNIQUE)
  content_type        TEXT NOT NULL    -- "audio/flac"
  audio_headers       BLOB            -- for CUE-split tracks: headers to prepend
  needs_headers       BOOLEAN NOT NULL
  start_byte_offset   INTEGER         -- CUE/FLAC: track start in shared file
  end_byte_offset     INTEGER         -- CUE/FLAC: track end in shared file
  pregap_ms           INTEGER         -- CUE/FLAC: INDEX 00 gap duration
  frame_offset_samples INTEGER        -- samples to skip after frame alignment
  exact_sample_count  INTEGER         -- for gapless playback trimming
  sample_rate         INTEGER NOT NULL
  bits_per_sample     INTEGER NOT NULL
  seektable_json      TEXT NOT NULL    -- dense frame-level seektable
  audio_data_start    INTEGER NOT NULL -- byte offset where audio data begins
  file_id             TEXT FK -> release_files
  created_at          TEXT NOT NULL
  _updated_at         TEXT NOT NULL    -- sync metadata
```

### `library_images` -- bae-managed metadata images

All images that bae creates and manages (as opposed to release files which are the user's data). One table, discriminated by `type`.

```
library_images
  id            TEXT PK          -- release_id for covers, artist_id for artists
  type          TEXT NOT NULL    -- "cover", "artist"
  content_type  TEXT NOT NULL    -- "image/jpeg", "image/png"
  file_size     INTEGER NOT NULL
  width         INTEGER
  height        INTEGER
  source        TEXT NOT NULL    -- "local", "musicbrainz", "discogs"
  source_url    TEXT             -- see below
  created_at    TEXT NOT NULL
  _updated_at   TEXT NOT NULL    -- sync metadata
```

File location is deterministic from the id: `images/{prefix}/{subprefix}/{id}` (same hash-based layout as `storage/`). No extension on disk -- content type is in the DB. No `source_path` needed -- the path is derived.

`source_url` values:
- MusicBrainz: selected Cover Art Archive image URL
- Discogs: image URL (e.g., `"https://i.discogs.com/..."`)
- Local (selected from release files): `"release://{relative_path}"` (e.g., `"release://Artwork/front.jpg"`)

## Image server

Images and release files are served over HTTP (axum, OS-assigned port, HMAC-signed URLs). Two endpoints:

- `/image/{id}` -- serves library images (covers, artist photos). Looks up `library_images WHERE id = ?`, reads `images/.../{id}`, serves with correct Content-Type.
- `/file/{file_id}` -- serves release files. Looks up `release_files WHERE id = ?`, reads from `source_path`, decrypts if needed, serves with correct Content-Type.

## Sync

Desktop is the single writer. After mutations, it pushes changesets to the cloud home (if configured). Other devices pull changesets and apply them with a conflict handler. Images are synced alongside the changesets that reference them. See `notes/03-sync.md` for details.

## Cover lifecycle

**Import with local cover**: user selects a cover from among the release's image files -> bae copies the bytes to `images/.../{release_id}`, inserts `library_images` row with `type = "cover"`, `source = "local"`, `source_url = "release://cover.jpg"`. The original image stays untouched in the release files. The cover is a copy -- bae can crop, resize, or optimize it without affecting the original. This is the same flow as a remotely fetched cover, just with a local source.

**Import with remote cover**: user selects MB/Discogs cover -> bae downloads, writes to `images/.../{release_id}`, inserts `library_images` row with source_url pointing back to the external source.

**Cover picker -- change to existing release image**: user picks a different image from the release's files -> bae reads from `source_path`, writes to `images/.../{release_id}`, upserts `library_images` row.

**Cover picker -- download new cover**: user picks from MB/Discogs -> download, write to `images/.../{release_id}`, upsert `library_images` row.

**Artist image fetch**: during import, fetch artist photo from Discogs -> write to `images/.../{artist_id}`, upsert `library_images` row with `type = "artist"`.

## First-run flows

### New library

On first run (no `~/.bae/active-library`), desktop shows a welcome screen. User picks "Create new library":

1. Generate a library UUID (e.g., `lib-111`)
2. Create `~/.bae/libraries/lib-111/`
3. Create empty `library.db`
4. Write `config.yaml`, write `~/.bae/active-library` -> `lib-111`
5. Re-exec binary -- desktop launches normally

`storage/` is empty -- user imports their first album, files go into `storage/ab/cd/{file_id}`.

### Restore from cloud home

User picks "Restore from cloud home" and provides cloud home credentials + encryption key:

1. Download + decrypt `snapshot.db.enc` (validates the key -- if decryption fails, wrong key)
2. Create `~/.bae/libraries/{library_id}/`
3. Write `config.yaml` (with cloud home config), keyring entries, `~/.bae/active-library` -> `{library_id}`
4. Download images from the cloud home
5. Pull and apply any changesets newer than the snapshot
6. Re-exec binary

Local `storage/` is empty -- release files stream from the cloud home. The user can optionally download files locally for offline playback.

### Going from local to cloud

1. User signs in with a cloud provider (OAuth) or enters S3 credentials
2. bae creates the cloud home folder/bucket (or uses an existing one)
3. bae generates encryption key if one doesn't exist, stores in keyring
4. bae pushes a full snapshot + all images + release files to the cloud home
5. Subsequent mutations push incremental changesets
6. Another of your devices can now connect to the cloud home via a restore code
