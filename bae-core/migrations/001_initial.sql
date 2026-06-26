-- bae's application schema. coven runs this (idempotently) after its own
-- bookkeeping migration when it opens the connection it owns, so every
-- `CREATE TABLE`/`CREATE INDEX` is `IF NOT EXISTS`: re-running over a
-- snapshot-bootstrapped database that already carries the schema is a no-op.
--
-- The `sync_cursors`, `sync_state`, and `cloud_outbox` bookkeeping tables are
-- owned by coven (created by coven's MIGRATION_SQL), not declared here.

CREATE TABLE IF NOT EXISTS artists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    sort_name TEXT,
    discogs_artist_id TEXT,
    musicbrainz_artist_id TEXT,

    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- Albums are aggregates over releases; identity lives on `release_identities`.
CREATE TABLE IF NOT EXISTS albums (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    -- Primary artist FK. Additional artists live in album_artists with position > 0.
    -- Nullable in SQLite because NOT NULL cannot be added to an existing column
    -- without recreating the table; the application layer treats it as required.
    artist_id TEXT REFERENCES artists(id),
    year INTEGER,
    -- The release that supplies the album's cover art and is shown by default.
    -- When NULL, callers fall back to the first release.
    primary_release_id TEXT,
    is_compilation BOOLEAN NOT NULL DEFAULT FALSE,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS album_artists (
    id TEXT PRIMARY KEY,
    album_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    UNIQUE(album_id, artist_id)
);

CREATE TABLE IF NOT EXISTS releases (
    id TEXT PRIMARY KEY,
    album_id TEXT NOT NULL,
    release_name TEXT,
    year INTEGER,
    disc_id TEXT,
    -- 'musicbrainz' | 'discogs' | 'file_tags'.
    metadata_source TEXT NOT NULL,
    -- Specific MB/Discogs release the metadata was seeded from. NULL
    -- when `metadata_source = 'file_tags'`.
    metadata_source_release_id TEXT,
    format TEXT,
    label TEXT,
    catalog_number TEXT,
    country TEXT,
    barcode TEXT,
    -- Shared, synced fact: is this release's audio in the cloud home (remote)
    -- or local to one device (local). A local release's in-place source
    -- folder lives in the device-local `release_local_source`, NOT here — it
    -- must not sync. A remote release's bytes live in coven's blob cache.
    remote BOOLEAN NOT NULL,
    source_folder_name TEXT,
    -- SHA-256 over the imported folder's categorized file structure (sorted
    -- relative paths + sizes). Location-independent content fingerprint: the
    -- same rip in any parent folder hashes the same. Used to recognize an
    -- already-imported folder and to pick the overwrite target on re-import.
    content_hash TEXT,
    -- Album-level loudness measured at import (EBU R128 integrated loudness over
    -- all tracks combined), in LUFS. NULL = not measured (a measurement failure,
    -- or imported before measurement existed). Playback derives a gain from this
    -- and a constant target; the stored value is the raw measurement, never a gain.
    album_loudness_lufs REAL,
    -- Album-level true peak as a LINEAR ratio (1.0 = 0 dBTP), the max across all
    -- tracks. NULL = not measured. Playback caps the album gain at 1.0/peak to
    -- prevent clipping.
    album_peak_linear REAL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
);

-- DEVICE-LOCAL local source: a row means "this release is LOCAL on this
-- device, with its files in place at `path`". It exists for exactly the local
-- releases (those with `releases.remote = 0`) and is the folder their audio
-- plays from. Not synced (no `_updated_at`, absent from SYNCED_TABLES) — an
-- local release is local to one device.
--
-- A REMOTE release has NO row here: its bytes live only in coven's blob cache
-- (`storage/pinned/` when kept local, else fetched into `storage/cache/` on
-- read), which coven owns. "Is it local" and "is it pinned" for a remote
-- release are answered by coven's cache, never by a column here.
CREATE TABLE IF NOT EXISTS release_local_source (
    release_id TEXT PRIMARY KEY REFERENCES releases (id) ON DELETE CASCADE,
    path       TEXT NOT NULL
);

-- Per-source identity rows. A release has 0+ rows: 0 = Unknown, 1+ = identified
-- (each row is independently Exact when source_release_id is set, Approximate
-- when NULL). Cross-source equivalences are encoded by a release having rows
-- in multiple sources.
CREATE TABLE IF NOT EXISTS release_identities (
    id                TEXT PRIMARY KEY,
    release_id        TEXT NOT NULL,
    source            TEXT NOT NULL,
    source_group_id   TEXT NOT NULL,
    source_release_id TEXT,
    _updated_at       TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    UNIQUE (release_id, source),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS tracks (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    title TEXT NOT NULL,
    side INTEGER NOT NULL,
    track_number INTEGER,
    duration_ms INTEGER,
    discogs_position TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS track_artists (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS release_files (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    content_type TEXT NOT NULL,
    -- Cloud object key for this file's remote blob, mirroring coven's
    -- BlobRef.cloud_path. NULL = the hashed-by-id layout (opaque homes); a
    -- value = the explicit readable key set when the file entered a browsable
    -- home (`{artist}/{album}/{filename}`). Synced, so every device addresses
    -- the blob the same way; computed once at upload time and never re-derived,
    -- so a metadata rename never moves the blob.
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
);

-- bits_per_sample is nullable: lossy codecs (MP3, AAC, etc.) don't expose a
-- bit depth via FFmpeg, and substituting a default would store a fabricated
-- value. NULL surfaces the absence to consumers.
-- A track is a sample window [start_sample, end_sample) into its backing file.
-- Standalone per-track files use (0, NULL) = the whole file; CUE tracks carry
-- the track's bounds. Playback decodes the file natively (FFmpeg) and seeks /
-- stops by sample -- there is no byte-range extraction or synthetic header.
CREATE TABLE IF NOT EXISTS audio_formats (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL UNIQUE,
    content_type TEXT NOT NULL,
    pregap_ms INTEGER,
    sample_rate INTEGER NOT NULL,
    bits_per_sample INTEGER,
    channels INTEGER NOT NULL,
    file_id TEXT REFERENCES release_files(id) ON DELETE SET NULL,
    start_sample INTEGER NOT NULL,
    end_sample INTEGER,
    end_byte INTEGER,
    -- Per-track loudness measured at import (EBU R128 integrated loudness over
    -- this track's sample window), in LUFS. NULL = not measured (decode/measure
    -- failure, or a near-silent track that has no usable loudness). Playback
    -- derives a gain from this and a constant target; the stored value is the
    -- raw measurement, never a gain.
    track_loudness_lufs REAL,
    -- Per-track true peak as a LINEAR ratio (1.0 = 0 dBTP), the max across
    -- channels. NULL = not measured. Playback caps the track gain at 1.0/peak
    -- to prevent clipping.
    track_peak_linear REAL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE
);

-- Column order here is the single source of truth for the changeset column
-- INDEX `BlobPlan` reads `cloud_path` at; `bae-core/src/sync/blob_plan.rs`
-- mirrors it in `LIBRARY_IMAGES_COLUMNS` / `LIBRARY_IMAGES_CLOUD_PATH_INDEX`,
-- with a guard test that fails loudly if this DDL and that index drift.
CREATE TABLE IF NOT EXISTS library_images (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    content_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    source_url TEXT,
    -- Cloud object key for this image's blob, mirroring coven's
    -- BlobRef.cloud_path. NULL = the hashed-by-id layout (opaque homes); a
    -- value = the explicit readable key (relative to the `images` namespace)
    -- set when the image entered a browsable home (cover:
    -- `{artist}/{album}/cover.{ext}`, artist: `{artist}/artist.{ext}`).
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS release_metadata (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    json TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    UNIQUE(release_id, source)
);

CREATE TABLE IF NOT EXISTS imports (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'preparing',
    release_id TEXT REFERENCES releases(id),
    album_title TEXT NOT NULL,
    artist_name TEXT NOT NULL,
    folder_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    error_message TEXT
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_artists_discogs_id ON artists (discogs_artist_id);
CREATE INDEX IF NOT EXISTS idx_artists_mb_id ON artists (musicbrainz_artist_id);
CREATE INDEX IF NOT EXISTS idx_artists_name ON artists (name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_albums_artist_id ON albums (artist_id);
CREATE INDEX IF NOT EXISTS idx_album_artists_album_id ON album_artists (album_id);
CREATE INDEX IF NOT EXISTS idx_album_artists_artist_id ON album_artists (artist_id);
CREATE INDEX IF NOT EXISTS idx_track_artists_track_id ON track_artists (track_id);
CREATE INDEX IF NOT EXISTS idx_track_artists_artist_id ON track_artists (artist_id);
CREATE INDEX IF NOT EXISTS idx_releases_album_id ON releases (album_id);
CREATE INDEX IF NOT EXISTS idx_release_identities_group
    ON release_identities (source, source_group_id);
CREATE INDEX IF NOT EXISTS idx_release_identities_release
    ON release_identities (source, source_release_id)
    WHERE source_release_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tracks_release_id ON tracks (release_id);
CREATE INDEX IF NOT EXISTS idx_release_files_release_id ON release_files (release_id);
CREATE INDEX IF NOT EXISTS idx_audio_formats_track_id ON audio_formats (track_id);
CREATE INDEX IF NOT EXISTS idx_library_images_type ON library_images (type);
CREATE INDEX IF NOT EXISTS idx_imports_status ON imports (status);
CREATE INDEX IF NOT EXISTS idx_imports_release_id ON imports (release_id);

CREATE TABLE IF NOT EXISTS attribution_names (
    pubkey_hex TEXT PRIMARY KEY,
    display_name TEXT NOT NULL
);

-- Device-local playback queue, restored after a restart on the same device.
-- NOT synced: no `_updated_at` and absent from `synced_tables()`, the same
-- device-local convention as coven's own bookkeeping tables. A single row
-- (id = 'current'). `source` is the context's release id (NULL for a single
-- track); `shuffle_seed` NULL means sequential, else shuffled with that seed.
CREATE TABLE IF NOT EXISTS playback_state (
    id               TEXT PRIMARY KEY,
    source           TEXT,
    shuffle_seed     INTEGER,
    cursor           INTEGER,
    manual           TEXT NOT NULL,
    repeat           TEXT NOT NULL,
    current_track_id TEXT,
    position_ms      INTEGER,
    volume           REAL NOT NULL,
    is_muted         INTEGER NOT NULL
);
