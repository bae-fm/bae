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
    -- Shared, synced fact (the coven gate column): is this release's audio in
    -- the cloud home (remote) or local to one device (local). A local release's
    -- in-place files are tracked by coven as external blob refs
    -- (`local_blob_refs`, coven's own device-local table), NOT here — they must
    -- not sync. A remote release's bytes live in coven's blob cache.
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

-- The device-local truth that a local release's files are in place is owned by
-- coven, not bae: a local release file is a coven *user-provided* blob, tracked
-- in coven's own device-local `local_blob_refs` table (blob_id = file id, the
-- path = `folder/original_filename`). bae registers/clears those refs through
-- coven's `register_external_blob` / `clear_external_blob`; coven flips them as
-- part of the make-Remote / make-Local transitions. There is no bae table here.

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

CREATE TABLE IF NOT EXISTS works (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    disambiguation TEXT,
    work_type TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS work_artists (
    id TEXT PRIMARY KEY,
    work_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (work_id) REFERENCES works (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    UNIQUE(work_id, artist_id, position)
);

CREATE TABLE IF NOT EXISTS work_parts (
    id TEXT PRIMARY KEY,
    parent_work_id TEXT NOT NULL,
    child_work_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (parent_work_id) REFERENCES works (id) ON DELETE CASCADE,
    FOREIGN KEY (child_work_id) REFERENCES works (id) ON DELETE CASCADE,
    UNIQUE(parent_work_id, child_work_id)
);

CREATE TABLE IF NOT EXISTS track_works (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL,
    work_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    FOREIGN KEY (work_id) REFERENCES works (id) ON DELETE CASCADE,
    UNIQUE(track_id, work_id)
);

CREATE TABLE IF NOT EXISTS release_artist_roles (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    source_credit TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    UNIQUE(release_id, artist_id, position, source)
);

CREATE TABLE IF NOT EXISTS track_artist_roles (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    source_credit TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    UNIQUE(track_id, artist_id, position, source)
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
    -- Byte this track's audio begins at within its backing file: the seektable
    -- checkpoint the playback seek lands on (computed at import by seeking to
    -- start_sample). NULL for a track starting at byte 0 (album's first track /
    -- whole-file track) — nothing to prefetch, the header read covers it.
    -- Playback fetches this window in parallel with the header probe so the
    -- track-start seek lands on buffered bytes instead of a second round-trip.
    start_byte INTEGER,
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

-- Album covers — the one small grid image bae produces per release, 1:1 with a
-- release (`id` IS the release id). A coven host-provided · CacheEager *asset*:
-- bae hands coven the bytes (`local_files::store("covers", id, …)`), coven owns
-- the on-device copy (its local store while Local, its cache while Remote). The
-- FK on `id` makes coven gate the cover as a child of its release and ride the
-- release's gate without keeping it alive (declared `.asset()`).
CREATE TABLE IF NOT EXISTS covers (
    -- The release id this cover belongs to (1:1), and the cover blob's id.
    id TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    source_url TEXT,
    -- Cloud object key for this cover's blob (relative to the `covers`
    -- namespace coven prepends), mirroring coven's BlobRef.cloud_path. NULL =
    -- the hashed-by-id layout (opaque homes); a value = the explicit readable
    -- key (`{album}/{release}/cover.{ext}`) on a browsable home.
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES releases (id) ON DELETE CASCADE
);

-- Artist images — bae-produced, 1:1 with an artist (`id` IS the artist id). A
-- coven host-provided · CacheEager *asset* of `artists`, same shape as `covers`.
CREATE TABLE IF NOT EXISTS artist_images (
    -- The artist id this image belongs to (1:1), and the image blob's id.
    id TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    source_url TEXT,
    -- Cloud object key for this image's blob (relative to the `artist_images`
    -- namespace coven prepends). NULL = hashed-by-id (opaque homes); a value =
    -- the readable `{artist}/artist.{ext}` key on a browsable home.
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES artists (id) ON DELETE CASCADE
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

CREATE INDEX IF NOT EXISTS idx_work_artists_artist ON work_artists(artist_id);
CREATE INDEX IF NOT EXISTS idx_work_artists_work ON work_artists(work_id);
CREATE INDEX IF NOT EXISTS idx_work_parts_parent ON work_parts(parent_work_id);
CREATE INDEX IF NOT EXISTS idx_work_parts_child ON work_parts(child_work_id);
CREATE INDEX IF NOT EXISTS idx_track_works_track ON track_works(track_id);
CREATE INDEX IF NOT EXISTS idx_track_works_work ON track_works(work_id);
CREATE INDEX IF NOT EXISTS idx_release_artist_roles_artist ON release_artist_roles(artist_id);
CREATE INDEX IF NOT EXISTS idx_release_artist_roles_release ON release_artist_roles(release_id);
CREATE INDEX IF NOT EXISTS idx_track_artist_roles_artist ON track_artist_roles(artist_id);
CREATE INDEX IF NOT EXISTS idx_track_artist_roles_track ON track_artist_roles(track_id);

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
