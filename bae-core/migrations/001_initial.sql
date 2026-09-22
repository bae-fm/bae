-- bae's application schema. coven runs this (idempotently) after its own
-- bookkeeping migration when it opens the connection it owns, so every
-- `CREATE TABLE`/`CREATE INDEX` is `IF NOT EXISTS`: re-running over a
-- snapshot-bootstrapped database that already carries the schema is a no-op.
--
-- coven's own bookkeeping tables (sync cursors, the cloud outbox, the circle
-- and store-write ledgers) are created by coven's MIGRATION_SQL, not here.
--
-- Sections: the library, playback, watched folders and their scans, import
-- candidates, identification, and the catalog documents lookups cached.

-- ── The library ───────────────────────────────────────────────────────────────

-- Every artist the library knows, whether credited on a release, a track, or a
-- work. The provider ids are what a later lookup matches an incoming artist to.
CREATE TABLE IF NOT EXISTS artists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    sort_name TEXT,
    discogs_artist_id TEXT,
    musicbrainz_artist_id TEXT,

    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artists_name ON artists (name COLLATE NOCASE);

CREATE INDEX IF NOT EXISTS idx_artists_discogs_id ON artists (discogs_artist_id);

CREATE INDEX IF NOT EXISTS idx_artists_mb_id ON artists (musicbrainz_artist_id);

-- One stored picture per artist, keyed by the artist it belongs to.
CREATE TABLE IF NOT EXISTS artist_images (
    -- The artist id this image belongs to (1:1).
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
    -- Content hash, as on release_files.hash.
    hash TEXT NOT NULL,
    -- The id of the coven blob holding this image's bytes — a new one per
    -- stored image, as on covers.blob_id.
    blob_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

-- Albums are aggregates over releases; a pressing's own facts live on its row
-- in `releases`.
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
    is_compilation INTEGER NOT NULL DEFAULT 0,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_albums_artist_id ON albums (artist_id);

-- The artists an album is credited to, in credit order.
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
) STRICT;

CREATE INDEX IF NOT EXISTS idx_album_artists_album_id ON album_artists (album_id);

CREATE INDEX IF NOT EXISTS idx_album_artists_artist_id ON album_artists (artist_id);

-- The compositions tracks perform, as MusicBrainz names them.
CREATE TABLE IF NOT EXISTS works (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    disambiguation TEXT,
    work_type TEXT,
    musicbrainz_work_id TEXT NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_works_mb_id ON works (musicbrainz_work_id);

-- Who wrote, arranged, or is otherwise credited on a work.
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
) STRICT;

CREATE INDEX IF NOT EXISTS idx_work_artists_work ON work_artists(work_id);

CREATE INDEX IF NOT EXISTS idx_work_artists_artist ON work_artists(artist_id);

-- A work that is part of a larger work, in the parent's order.
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
) STRICT;

CREATE INDEX IF NOT EXISTS idx_work_parts_parent ON work_parts(parent_work_id);

CREATE INDEX IF NOT EXISTS idx_work_parts_child ON work_parts(child_work_id);

-- One pressing of an album: the physical or digital edition whose audio the
-- library holds.
CREATE TABLE IF NOT EXISTS releases (
    id TEXT PRIMARY KEY,
    album_id TEXT NOT NULL,
    release_name TEXT,
    year INTEGER,
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
    remote INTEGER NOT NULL,
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
    -- Whether the stored metadata was read off the folder's own file tags
    -- rather than a catalog record.
    draft_from_tags INTEGER NOT NULL DEFAULT 0 CHECK (draft_from_tags IN (0, 1)),
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_releases_album_id ON releases (album_id);

CREATE INDEX IF NOT EXISTS idx_releases_content_hash
    ON releases (content_hash)
    WHERE content_hash IS NOT NULL;

-- One stored cover per release, keyed by the release it belongs to.
CREATE TABLE IF NOT EXISTS covers (
    -- The release id this cover belongs to (1:1).
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
    -- Content hash, as on release_files.hash.
    hash TEXT NOT NULL,
    -- The id of the coven blob holding this cover's bytes. Distinct from the
    -- row id (which is the release id and cannot move): coven names one
    -- immutable byte-string per (namespace, blob id), so replacing a cover
    -- repoints the row at a NEW blob id rather than writing new bytes under
    -- the old one — which coven refuses (`BlobAlreadyReferenced`).
    blob_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

-- The files a release's audio and artwork are stored as, with the facts of the
-- audio they were imported from.
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
    -- Lowercase-hex SHA-256 of the blob's plaintext (coven's
    -- `BlobDecl::hash_column`), signed on the row alongside the declared size
    -- and verified against the decrypted bytes on a Remote fetch. NOT NULL:
    -- coven reads it off every blob-bearing row and refuses a row without one,
    -- so a hashless blob is not a state this schema can hold.
    hash TEXT NOT NULL,
    -- What the file held before import rewrote it: either its own audio
    -- ('file') or one slice of a CUE-described disc ('cue'). All seven facts
    -- are present together or all absent, and which of bits-per-sample and
    -- bitrate is stated follows from the codec.
    source_audio_layout TEXT CHECK (source_audio_layout IS NULL OR source_audio_layout IN ('file', 'cue')),
    source_audio_content_type TEXT,
    source_audio_duration_ms INTEGER CHECK (source_audio_duration_ms IS NULL OR source_audio_duration_ms >= 0),
    source_audio_sample_rate_hz INTEGER CHECK (source_audio_sample_rate_hz IS NULL OR source_audio_sample_rate_hz > 0),
    source_audio_bits_per_sample INTEGER CHECK (source_audio_bits_per_sample IS NULL OR source_audio_bits_per_sample > 0),
    source_audio_bitrate_kbps INTEGER CHECK (source_audio_bitrate_kbps IS NULL OR source_audio_bitrate_kbps > 0),
    source_audio_channels INTEGER CHECK (
        (source_audio_layout IS NULL
            AND source_audio_content_type IS NULL
            AND source_audio_duration_ms IS NULL
            AND source_audio_sample_rate_hz IS NULL
            AND source_audio_bits_per_sample IS NULL
            AND source_audio_bitrate_kbps IS NULL
            AND source_audio_channels IS NULL)
        OR (
            source_audio_channels IS NOT NULL
            AND source_audio_channels > 0
            AND source_audio_content_type IS NOT NULL
            AND source_audio_duration_ms IS NOT NULL
            AND source_audio_sample_rate_hz IS NOT NULL
            AND (
                (source_audio_content_type IN (
                    'audio/flac', 'audio/x-ape', 'audio/alac', 'audio/pcm',
                    'audio/wavpack', 'audio/dsd'
                ) AND source_audio_bits_per_sample IS NOT NULL
                    AND source_audio_bitrate_kbps IS NULL)
                OR
                (source_audio_content_type IN (
                    'audio/mpeg', 'audio/aac', 'audio/opus', 'audio/vorbis'
                ) AND source_audio_bits_per_sample IS NULL
                    AND source_audio_bitrate_kbps IS NOT NULL)
            )
        )
    ),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_files_release_id ON release_files (release_id);

-- Who is credited on a release, and in what role, as the catalog stated it.
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
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_artist_roles_release ON release_artist_roles(release_id);

CREATE INDEX IF NOT EXISTS idx_release_artist_roles_artist ON release_artist_roles(artist_id);

-- The catalog entries that describe a release: one per catalog, naming either
-- the pressing itself or the album it belongs to. Exactly one per release may
-- be the record the stored metadata was read from.
CREATE TABLE IF NOT EXISTS release_records (
    id          TEXT NOT NULL PRIMARY KEY,
    release_id  TEXT NOT NULL,
    catalog     TEXT NOT NULL,
    key         TEXT NOT NULL,
    album_key   TEXT,
    url         TEXT NOT NULL CHECK (url <> ''),
    reads_draft INTEGER NOT NULL CHECK (reads_draft IN (0, 1)),
    _updated_at TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('pressing', 'album')),
    CHECK (kind = 'pressing' OR (album_key IS NULL AND reads_draft = 0)),
    UNIQUE (release_id, catalog),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_records_catalog_key ON release_records (catalog, key) WHERE kind = 'pressing';

CREATE INDEX IF NOT EXISTS idx_release_records_catalog_album ON release_records
    (catalog, CASE WHEN kind = 'album' THEN key ELSE album_key END);

CREATE UNIQUE INDEX IF NOT EXISTS idx_release_records_reads_draft
    ON release_records (release_id) WHERE reads_draft = 1;

-- The tracks of a release, in playing order.
CREATE TABLE IF NOT EXISTS tracks (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    title TEXT NOT NULL,
    track_number INTEGER,
    duration_ms INTEGER,
    discogs_position TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- Playing order within the release, counted from zero over every side.
    position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    -- The side or disc this track sits on, where the pressing has them.
    side INTEGER,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_tracks_release_id ON tracks (release_id);

CREATE UNIQUE INDEX IF NOT EXISTS tracks_release_position ON tracks(release_id, position);

-- The artists a track is credited to, in credit order.
CREATE TABLE IF NOT EXISTS track_artists (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_track_artists_track_id ON track_artists (track_id);

CREATE INDEX IF NOT EXISTS idx_track_artists_artist_id ON track_artists (artist_id);

-- Who is credited on a track, and in what role, as the catalog stated it.
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
) STRICT;

CREATE INDEX IF NOT EXISTS idx_track_artist_roles_track ON track_artist_roles(track_id);

CREATE INDEX IF NOT EXISTS idx_track_artist_roles_artist ON track_artist_roles(artist_id);

-- The works a track performs, in the order the track performs them.
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
) STRICT;

CREATE INDEX IF NOT EXISTS idx_track_works_track ON track_works(track_id);

CREATE INDEX IF NOT EXISTS idx_track_works_work ON track_works(work_id);

-- How a track's audio is laid out in the stored files, and the loudness it was
-- measured at.
CREATE TABLE IF NOT EXISTS audio_formats (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL UNIQUE,
    content_type TEXT NOT NULL,
    pregap_ms INTEGER,
    generated_pregap_ms INTEGER,
    pregap_samples INTEGER,
    generated_pregap_samples INTEGER,
    sample_rate INTEGER NOT NULL,
    bits_per_sample INTEGER,
    channels INTEGER NOT NULL,
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
) STRICT;

CREATE INDEX IF NOT EXISTS idx_audio_formats_track_id ON audio_formats (track_id);

-- The consecutive spans of a file a track's audio is read from: its pregap,
-- then its main body.
CREATE TABLE IF NOT EXISTS audio_format_segments (
    id TEXT PRIMARY KEY,
    audio_format_id TEXT NOT NULL,
    segment_index INTEGER NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('audio_pregap', 'main')),
    file_id TEXT NOT NULL,
    start_sample INTEGER NOT NULL,
    end_sample INTEGER,
    start_byte INTEGER,
    end_byte INTEGER,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (audio_format_id, segment_index),
    FOREIGN KEY (audio_format_id) REFERENCES audio_formats (id) ON DELETE CASCADE,
    FOREIGN KEY (file_id) REFERENCES release_files(id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_audio_format_segments_format_id ON audio_format_segments (audio_format_id);

-- ── Playback ──────────────────────────────────────────────────────────────────

-- The one row describing what this device was playing, so a restart resumes it.
-- Device-local: never synced.
CREATE TABLE IF NOT EXISTS playback_state (
    id               TEXT PRIMARY KEY,
    source           TEXT,
    -- Whether the context lane was shuffled. Restore refills the lane from
    -- `source` and permutes it afresh; the session's shuffled order is not
    -- stored. NULL exactly when `source` is (no context playing).
    shuffled         INTEGER,
    manual           TEXT NOT NULL,
    repeat           TEXT NOT NULL,
    current_track_id TEXT,
    position_ms      INTEGER,
    volume           REAL NOT NULL,
    is_muted         INTEGER NOT NULL
);

-- ── Watched folders and their scans ───────────────────────────────────────────

-- The folders the desktop app watches for importable releases, in the order
-- the user arranged them.
CREATE TABLE IF NOT EXISTS watched_import_folders (
    path      TEXT PRIMARY KEY,
    position  INTEGER NOT NULL UNIQUE CHECK (position >= 0)
) STRICT;

-- Whether a folder that holds several release-looking subfolders is one
-- release or several.
CREATE TABLE IF NOT EXISTS folder_release_decisions (
    watched_folder_path  TEXT NOT NULL,
    relative_folder_path TEXT NOT NULL,
    decision             TEXT NOT NULL CHECK (
        decision IN ('combine_as_one_release', 'keep_as_separate_releases')
    ),
    -- Who decided. The scan reads a folder its own way when nothing is stored
    -- and records that as 'heuristic'; the user's own answer replaces it as
    -- 'user' and is never read over again.
    author               TEXT NOT NULL CHECK (author IN ('user', 'heuristic')),
    PRIMARY KEY (watched_folder_path, relative_folder_path),
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path)
        ON DELETE CASCADE
) STRICT;

-- The candidates the user dismissed, so a later scan does not offer them again.
CREATE TABLE IF NOT EXISTS skipped_import_candidates (
    watched_folder_path    TEXT NOT NULL,
    relative_candidate_path TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, relative_candidate_path),
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path)
        ON DELETE CASCADE
) STRICT;

-- The one row handing out scan generations, so every root's generation is
-- durable before its traversal begins.
CREATE TABLE IF NOT EXISTS folder_scan_generation_sequence (
    singleton       INTEGER PRIMARY KEY CHECK (singleton = 1),
    last_generation INTEGER NOT NULL CHECK (last_generation >= 0)
) STRICT;

INSERT OR IGNORE INTO folder_scan_generation_sequence (singleton, last_generation)
VALUES (1, 0);

-- Device-local cache of the last observed scan of each watched folder. Entries
-- are written as they are discovered; successful completion removes entries not
-- seen in that generation in the same transaction that marks the root complete.
-- A failed or interrupted scan keeps both previously known and newly discovered
-- entries.
CREATE TABLE IF NOT EXISTS folder_scan_roots (
    watched_folder_path TEXT PRIMARY KEY,
    generation          INTEGER NOT NULL CHECK (generation >= 0),
    status              TEXT NOT NULL CHECK (status IN ('scanning', 'complete', 'failed')),
    error               TEXT,
    CHECK (
        (status = 'failed' AND error IS NOT NULL)
        OR
        (status != 'failed' AND error IS NULL)
    ),
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path)
        ON DELETE CASCADE
) STRICT;

-- The directories seen under a watched folder and when each last changed, so a
-- rescan can skip the ones that did not.
CREATE TABLE IF NOT EXISTS folder_scan_directory (
    watched_folder_path TEXT NOT NULL,
    path                TEXT NOT NULL,
    modified_at         INTEGER NOT NULL,
    PRIMARY KEY (watched_folder_path, path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE
) STRICT;

-- One release-looking folder the scan found, or one combination of them.
CREATE TABLE IF NOT EXISTS scan_candidate (
    watched_folder_path            TEXT NOT NULL,
    path                           TEXT NOT NULL,
    generation                     INTEGER NOT NULL CHECK (generation >= 0),
    kind                           TEXT NOT NULL CHECK (kind IN ('tentative', 'valid', 'invalid')),
    name                           TEXT NOT NULL,
    display_path                   TEXT NOT NULL,
    file_root                      TEXT,
    scope                          TEXT CHECK (scope IS NULL OR scope IN ('direct', 'recursive')),
    content_hash                   TEXT,
    file_edit_revision             INTEGER NOT NULL DEFAULT 0 CHECK (file_edit_revision >= 0),
    combine_ancestor_relative_path TEXT,
    invalid_reason                 TEXT CHECK (invalid_reason IS NULL OR invalid_reason IN ('corrupt_audio', 'corrupt_image', 'no_valid_audio')),
    invalid_reason_path            TEXT,
    first_seen_at                  INTEGER,
    source_date                    INTEGER,
    source_date_kind               TEXT CHECK ((source_date IS NULL AND source_date_kind IS NULL)
        OR (source_date IS NOT NULL AND source_date_kind IS NOT NULL
            AND source_date_kind IN ('added_to_directory', 'created'))),
    source_kind                    TEXT NOT NULL DEFAULT 'folder' CHECK (source_kind IN ('folder', 'combination')),
    PRIMARY KEY (watched_folder_path, path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE,
    CHECK ((kind = 'invalid') = (invalid_reason IS NOT NULL)),
    CHECK ((kind = 'invalid') = (file_root IS NULL AND scope IS NULL AND content_hash IS NULL)),
    CHECK ((invalid_reason IN ('corrupt_audio', 'corrupt_image')) = (invalid_reason_path IS NOT NULL))
) STRICT;

CREATE INDEX IF NOT EXISTS idx_scan_candidate_path ON scan_candidate (path);

CREATE INDEX IF NOT EXISTS idx_scan_candidate_content_hash
    ON scan_candidate (content_hash) WHERE content_hash IS NOT NULL;

-- Every file of a candidate folder, the role the scan read it as, and the audio
-- facts probing found in it.
CREATE TABLE IF NOT EXISTS scan_candidate_file (
    watched_folder_path   TEXT NOT NULL,
    candidate_path        TEXT NOT NULL,
    relative_path         TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    absolute_path         TEXT NOT NULL,
    size                  INTEGER NOT NULL CHECK (size >= 0),
    modified_at_ns        INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    audio_content_type    TEXT,
    audio_duration_ms     INTEGER CHECK (audio_duration_ms IS NULL OR audio_duration_ms >= 0),
    audio_sample_rate_hz  INTEGER CHECK (audio_sample_rate_hz IS NULL OR audio_sample_rate_hz > 0),
    audio_bits_per_sample INTEGER CHECK (audio_bits_per_sample IS NULL OR audio_bits_per_sample > 0),
    audio_bitrate_kbps    INTEGER CHECK (audio_bitrate_kbps IS NULL OR audio_bitrate_kbps > 0),
    audio_channels        INTEGER CHECK (audio_channels IS NULL OR audio_channels > 0),
    file_name             TEXT NOT NULL,
    dir_prefix            TEXT,
    proposed_audio        INTEGER NOT NULL CHECK (proposed_audio IN (0, 1)),
    role                  TEXT NOT NULL CHECK (role IN ('audio', 'track_sheet', 'artwork', 'document', 'other')),
    sheet_binding         TEXT CHECK (sheet_binding IS NULL OR sheet_binding IN ('resolved', 'override', 'unresolved', 'refused_codec')),
    sheet_binding_codec   TEXT,
    sheet_disc            TEXT CHECK (sheet_disc IS NULL OR sheet_disc IN ('disc', 'ignored')),
    sheet_disc_number     INTEGER CHECK (sheet_disc_number IS NULL OR sheet_disc_number >= 1),
    PRIMARY KEY (watched_folder_path, candidate_path, relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path) REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE,
    CHECK ((role = 'track_sheet') = (sheet_binding IS NOT NULL AND sheet_disc IS NOT NULL)),
    CHECK ((sheet_binding = 'refused_codec') = (sheet_binding_codec IS NOT NULL)),
    CHECK ((sheet_disc = 'disc') = (sheet_disc_number IS NOT NULL)),
    CHECK (
        (proposed_audio = 0 AND audio_content_type IS NULL AND audio_duration_ms IS NULL
            AND audio_sample_rate_hz IS NULL AND audio_bits_per_sample IS NULL
            AND audio_bitrate_kbps IS NULL AND audio_channels IS NULL)
        OR
        (proposed_audio = 1 AND audio_content_type IS NOT NULL AND audio_duration_ms IS NOT NULL
            AND audio_sample_rate_hz IS NOT NULL AND audio_channels IS NOT NULL
            AND (
                (audio_content_type IN (
                    'audio/flac', 'audio/x-ape', 'audio/alac', 'audio/pcm',
                    'audio/wavpack', 'audio/dsd'
                ) AND audio_bits_per_sample IS NOT NULL AND audio_bitrate_kbps IS NULL)
                OR
                (audio_content_type IN (
                    'audio/mpeg', 'audio/aac', 'audio/opus', 'audio/vorbis'
                ) AND audio_bits_per_sample IS NULL AND audio_bitrate_kbps IS NOT NULL)
            ))
    )
) STRICT;

-- What a candidate's files were probed at, and the cover its tags carried.
CREATE TABLE IF NOT EXISTS scan_candidate_tag_snapshot (
    watched_folder_path                 TEXT NOT NULL,
    candidate_path                      TEXT NOT NULL,
    scan_generation                     INTEGER NOT NULL CHECK (scan_generation >= 0),
    file_edit_revision                  INTEGER NOT NULL CHECK (file_edit_revision >= 0),
    embedded_cover_source_relative_path TEXT,
    embedded_cover_content_type         TEXT,
    embedded_cover_data                 BLOB,
    PRIMARY KEY (watched_folder_path, candidate_path),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE,
    CHECK (
        (embedded_cover_source_relative_path IS NULL
            AND embedded_cover_content_type IS NULL AND embedded_cover_data IS NULL)
        OR
        (embedded_cover_source_relative_path IS NOT NULL
            AND embedded_cover_content_type IS NOT NULL AND embedded_cover_data IS NOT NULL)
    )
) STRICT;

-- The tags each audio file of a candidate carried when it was probed.
CREATE TABLE IF NOT EXISTS scan_candidate_file_tag (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    relative_path       TEXT NOT NULL,
    file_size           INTEGER NOT NULL CHECK (file_size >= 0),
    modified_at_ns      INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    title               TEXT,
    track_artist        TEXT,
    album_title         TEXT,
    album_artist        TEXT,
    year                INTEGER,
    track_number        INTEGER,
    disc_number         INTEGER,
    PRIMARY KEY (watched_folder_path, candidate_path, relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate_tag_snapshot (watched_folder_path, candidate_path) ON DELETE CASCADE,
    FOREIGN KEY (watched_folder_path, candidate_path, relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE
) STRICT;

-- The subfolder boundaries a candidate was resolved across, and how each was
-- decided.
CREATE TABLE IF NOT EXISTS scan_candidate_resolved_boundary (
    watched_folder_path  TEXT NOT NULL,
    candidate_path       TEXT NOT NULL,
    position             INTEGER NOT NULL CHECK (position >= 0),
    relative_folder_path TEXT NOT NULL,
    decision             TEXT NOT NULL CHECK (decision IN ('combine_as_one_release', 'keep_as_separate_releases')),
    name                 TEXT NOT NULL,
    display_path         TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, position),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE
) STRICT;

-- A CUE sheet found beside a candidate's audio, as parsed.
CREATE TABLE IF NOT EXISTS scan_cue_sheet (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    sheet_relative_path TEXT NOT NULL,
    title               TEXT,
    performer           TEXT,
    catalog             TEXT,
    date                TEXT,
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE
) STRICT;

-- One track of a CUE sheet, with the span and pregap it declares.
CREATE TABLE IF NOT EXISTS scan_cue_track (
    watched_folder_path         TEXT NOT NULL,
    candidate_path              TEXT NOT NULL,
    sheet_relative_path         TEXT NOT NULL,
    position                    INTEGER NOT NULL CHECK (position >= 0),
    number                      INTEGER NOT NULL,
    mode                        TEXT NOT NULL CHECK (mode IN ('audio', 'other')),
    mode_other                  TEXT,
    title                       TEXT,
    performer                   TEXT,
    file_reference              TEXT NOT NULL,
    start_cue_frames            INTEGER NOT NULL CHECK (start_cue_frames >= 0),
    end_cue_frames              INTEGER CHECK (end_cue_frames IS NULL OR end_cue_frames >= 0),
    pregap_kind                 TEXT NOT NULL CHECK (pregap_kind IN ('none', 'audio', 'silence')),
    pregap_frames               INTEGER CHECK (pregap_frames IS NULL OR pregap_frames >= 0),
    pregap_index_number         INTEGER,
    pregap_index_file_reference TEXT,
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path, position),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_cue_sheet (watched_folder_path, candidate_path, sheet_relative_path) ON DELETE CASCADE,
    CHECK ((mode = 'other') = (mode_other IS NOT NULL)),
    CHECK ((pregap_kind = 'none') = (pregap_frames IS NULL)),
    CHECK ((pregap_kind = 'audio') = (pregap_index_number IS NOT NULL AND pregap_index_file_reference IS NOT NULL))
) STRICT;

-- Every INDEX line of a CUE track, in the order the sheet stated them.
CREATE TABLE IF NOT EXISTS scan_cue_index (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    sheet_relative_path TEXT NOT NULL,
    track_position      INTEGER NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    number              INTEGER NOT NULL,
    frames              INTEGER NOT NULL CHECK (frames >= 0),
    file_reference      TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path, track_position, position),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path, track_position)
        REFERENCES scan_cue_track (watched_folder_path, candidate_path, sheet_relative_path, position) ON DELETE CASCADE
) STRICT;

-- Which audio file each FILE reference of a CUE sheet resolved to.
CREATE TABLE IF NOT EXISTS scan_sheet_audio_file (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    sheet_relative_path TEXT NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    file_reference      TEXT NOT NULL,
    audio_relative_path TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path, position),
    UNIQUE (watched_folder_path, candidate_path, sheet_relative_path, file_reference),
    UNIQUE (watched_folder_path, candidate_path, sheet_relative_path, audio_relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE,
    FOREIGN KEY (watched_folder_path, candidate_path, audio_relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE
) STRICT;

-- Several scanned folders the user joined into one candidate.
CREATE TABLE IF NOT EXISTS candidate_combination (
    candidate_key TEXT PRIMARY KEY,
    watched_folder_path TEXT NOT NULL
        REFERENCES watched_import_folders (path) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    skipped INTEGER NOT NULL DEFAULT 0 CHECK (skipped IN (0, 1)),
    created_at INTEGER NOT NULL,
    error TEXT
) STRICT;

-- One member folder of a combination, with the discs and tracks it contributes.
CREATE TABLE IF NOT EXISTS candidate_combination_member (
    combination_key TEXT NOT NULL
        REFERENCES candidate_combination (candidate_key) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    candidate_key TEXT NOT NULL UNIQUE,
    watched_folder_path TEXT NOT NULL,
    folder_name TEXT NOT NULL,
    file_prefix TEXT NOT NULL,
    first_disc INTEGER NOT NULL CHECK (first_disc >= 1),
    disc_count INTEGER NOT NULL CHECK (disc_count >= 1),
    track_count INTEGER NOT NULL CHECK (track_count >= 1),
    PRIMARY KEY (combination_key, position)
) STRICT;

CREATE INDEX IF NOT EXISTS candidate_combination_member_by_root
    ON candidate_combination_member (watched_folder_path);

-- A combination outlives neither its watched folder nor its member folders: it
-- is dropped when the root goes, and marked unusable when a member changes.

CREATE TRIGGER IF NOT EXISTS remove_root_combinations BEFORE DELETE ON watched_import_folders
BEGIN
    DELETE FROM candidate_combination
    WHERE candidate_key IN (
        SELECT combination_key FROM candidate_combination_member
        WHERE watched_folder_path = OLD.path
    );
END;

CREATE TRIGGER IF NOT EXISTS invalidate_combination_source_delete BEFORE DELETE ON scan_candidate
WHEN OLD.source_kind = 'folder'
BEGIN
    UPDATE candidate_combination
    SET error = 'Source folder changed or disappeared: ' || OLD.name
    WHERE candidate_key IN (
        SELECT combination_key FROM candidate_combination_member WHERE candidate_key = OLD.path
    );
END;

CREATE TRIGGER IF NOT EXISTS invalidate_combination_source_edit AFTER UPDATE OF content_hash, file_edit_revision ON scan_candidate
WHEN OLD.source_kind = 'folder'
    AND (NEW.content_hash IS NOT OLD.content_hash OR NEW.file_edit_revision != OLD.file_edit_revision)
BEGIN
    UPDATE candidate_combination
    SET error = 'Source folder changed: ' || OLD.name
    WHERE candidate_key IN (
        SELECT combination_key FROM candidate_combination_member WHERE candidate_key = OLD.path
    );
END;

CREATE TRIGGER IF NOT EXISTS remove_combination_candidate AFTER DELETE ON candidate_combination
BEGIN
    DELETE FROM scan_candidate
    WHERE source_kind = 'combination' AND path = OLD.candidate_key;
END;

-- ── Import candidates ─────────────────────────────────────────────────────────

-- One folder being imported, named by the hash of its file structure. Every
-- other candidate table hangs off this one.
CREATE TABLE IF NOT EXISTS import_candidate_state (
    content_hash      TEXT PRIMARY KEY,
    -- Where the candidate was last seen. Not identity, not authoritative.
    folder_path       TEXT NOT NULL,
    -- Advances with every metadata-draft or selected-cover mutation. Commands
    -- return this value so a surface can wait for the exact committed detail.
    metadata_revision INTEGER NOT NULL DEFAULT 0 CHECK (metadata_revision >= 0),
    -- Advances with every file decision, so a verdict derived from an older
    -- shape is refused.
    edit_revision     INTEGER NOT NULL DEFAULT 0 CHECK (edit_revision >= 0)
) STRICT;

-- Which watched folders a candidate was found under — more than one when the
-- same folder is watched twice.
CREATE TABLE IF NOT EXISTS import_candidate_watched_root (
    content_hash        TEXT NOT NULL,
    watched_folder_path TEXT NOT NULL,
    PRIMARY KEY (content_hash, watched_folder_path),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS import_candidate_watched_root_by_root
    ON import_candidate_watched_root (watched_folder_path);

-- The release-level metadata draft the import will write.
CREATE TABLE IF NOT EXISTS import_candidate_edit (
    content_hash   TEXT PRIMARY KEY,
    album_title    TEXT NOT NULL,
    album_year     TEXT NOT NULL,
    year           TEXT NOT NULL,
    format         TEXT NOT NULL,
    label          TEXT NOT NULL,
    catalog_number TEXT NOT NULL,
    country        TEXT NOT NULL,
    barcode        TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The album artists of the draft: either an artist the library already holds
-- or a new one to create.
CREATE TABLE IF NOT EXISTS import_candidate_album_artist_assignment (
    content_hash          TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('existing', 'new')),
    artist_id             TEXT,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE RESTRICT,
    CHECK (
        (assignment_kind = 'existing' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL)
        OR
        (assignment_kind = 'new' AND artist_id IS NULL AND name IS NOT NULL AND name <> '')
    )
) STRICT;

-- The tracks of the draft, each bound to the file (or CUE slice) it plays.
CREATE TABLE IF NOT EXISTS import_candidate_track (
    content_hash           TEXT NOT NULL,
    track_id               TEXT NOT NULL,
    position               INTEGER NOT NULL CHECK (position >= 0),
    title                  TEXT NOT NULL,
    artist_assignment_kind TEXT NOT NULL CHECK (artist_assignment_kind IN ('album_artists', 'explicit')),
    side                   INTEGER,
    track_number           INTEGER NOT NULL,
    source_index           INTEGER CHECK (source_index IS NULL OR source_index >= 0),
    file_kind              TEXT NOT NULL CHECK (file_kind IN ('standalone', 'sheet_slice')),
    file_id                TEXT NOT NULL,
    sheet_id               TEXT,
    slice_index            INTEGER CHECK (slice_index IS NULL OR slice_index >= 0),
    PRIMARY KEY (content_hash, track_id),
    UNIQUE (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    CHECK (
        (file_kind = 'standalone' AND file_id IS NOT NULL AND sheet_id IS NULL AND slice_index IS NULL)
        OR (file_kind = 'sheet_slice' AND file_id IS NOT NULL AND sheet_id IS NOT NULL AND slice_index IS NOT NULL)
    )
) STRICT;

-- The per-track artists of the draft, where a track does not take the album's.
CREATE TABLE IF NOT EXISTS import_candidate_track_artist_assignment (
    content_hash          TEXT NOT NULL,
    track_id              TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('existing', 'new')),
    artist_id             TEXT,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (content_hash, track_id, position),
    FOREIGN KEY (content_hash, track_id)
        REFERENCES import_candidate_track (content_hash, track_id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE RESTRICT,
    CHECK (
        (assignment_kind = 'existing' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL)
        OR
        (assignment_kind = 'new' AND artist_id IS NULL AND name IS NOT NULL AND name <> '')
    )
) STRICT;

-- The user's answers about a candidate's files: what a file is, and which disc
-- a CUE sheet describes.
CREATE TABLE IF NOT EXISTS import_candidate_file_edit (
    content_hash TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    role_choice TEXT CHECK (role_choice IS NULL OR role_choice IN ('audio', 'not_a_track')),
    sheet_disc TEXT CHECK (sheet_disc IS NULL OR sheet_disc IN ('disc', 'ignored')),
    sheet_disc_number INTEGER CHECK (sheet_disc_number IS NULL OR sheet_disc_number >= 1),
    PRIMARY KEY (content_hash, relative_path),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state(content_hash) ON DELETE CASCADE,
    CHECK ((sheet_disc = 'disc') = (sheet_disc_number IS NOT NULL)),
    CHECK (role_choice IS NOT NULL OR sheet_disc IS NOT NULL)
) STRICT;

-- Which audio file the user bound each FILE reference of a CUE sheet to.
CREATE TABLE IF NOT EXISTS import_candidate_sheet_reference (
    content_hash TEXT NOT NULL,
    sheet_id TEXT NOT NULL,
    file_reference TEXT NOT NULL,
    file_id TEXT,
    PRIMARY KEY (content_hash, sheet_id, file_reference),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state(content_hash) ON DELETE CASCADE
) STRICT;

-- The cover the import will write: a file of the folder, a tag's embedded
-- image, or one offered by a catalog.
CREATE TABLE IF NOT EXISTS import_candidate_cover (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('local', 'remote', 'embedded')),
    file_id      TEXT,
    url          TEXT,
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((kind IN ('local', 'embedded')) = (file_id IS NOT NULL)),
    CHECK ((kind = 'remote') = (url IS NOT NULL AND source IS NOT NULL))
) STRICT;

-- The bytes of a catalog-offered cover, fetched before the import runs.
CREATE TABLE IF NOT EXISTS import_candidate_remote_cover_asset (
    content_hash TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    bytes        BLOB NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_cover (content_hash) ON DELETE CASCADE
) STRICT;

-- The Discogs artists the draft credits, so their pictures can be fetched.
CREATE TABLE IF NOT EXISTS import_candidate_source_artist (
    content_hash      TEXT NOT NULL,
    discogs_artist_id TEXT NOT NULL,
    PRIMARY KEY (content_hash, discogs_artist_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The picture fetched for one credited artist, or the settled answer that
-- there is none.
CREATE TABLE IF NOT EXISTS import_candidate_artist_asset (
    content_hash      TEXT NOT NULL,
    discogs_artist_id TEXT NOT NULL,
    answer            TEXT NOT NULL CHECK (answer IN ('image', 'nothing')),
    source_url        TEXT,
    content_type      TEXT,
    bytes             BLOB,
    PRIMARY KEY (content_hash, discogs_artist_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK (
        (answer = 'image' AND source_url IS NOT NULL AND content_type IS NOT NULL AND bytes IS NOT NULL)
        OR
        (answer = 'nothing' AND source_url IS NULL AND content_type IS NULL AND bytes IS NULL)
    )
) STRICT;

-- The candidates whose remote assets are all fetched and ready to import.
CREATE TABLE IF NOT EXISTS import_candidate_asset_preparation (
    content_hash TEXT PRIMARY KEY,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- Where the draft was read from, and who asked for that reading.
CREATE TABLE IF NOT EXISTS import_candidate_draft_provenance (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('external_release', 'file_tags')),
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    release_id   TEXT,
    author       TEXT NOT NULL CHECK (author IN ('user', 'identification')),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    -- Only a release names a release; File Tags names the candidate's own files.
    CHECK ((kind = 'external_release') = (source IS NOT NULL)),
    CHECK ((kind = 'external_release') = (release_id IS NOT NULL)),
    -- Identification only ever concludes a release. Reading a folder's own tags
    -- is something a person asks for.
    CHECK (author != 'identification' OR kind = 'external_release')
) STRICT;

-- The releases in the other catalogs that the draft's own record links to.
CREATE TABLE IF NOT EXISTS import_candidate_provenance_partner (
    content_hash TEXT NOT NULL,
    source       TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id   TEXT NOT NULL,
    PRIMARY KEY (content_hash, source),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_draft_provenance (content_hash) ON DELETE CASCADE
) STRICT;

-- The catalog documents the draft was built from, stored whole so the draft
-- can be re-read without asking the provider again.
CREATE TABLE IF NOT EXISTS import_candidate_applied_source (
    content_hash TEXT PRIMARY KEY,
    snapshot TEXT NOT NULL CHECK (json_valid(snapshot)),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- What the import pane is showing for this candidate and what is typed in its
-- search fields.
CREATE TABLE IF NOT EXISTS import_candidate_session (
    content_hash   TEXT PRIMARY KEY,
    presentation   TEXT NOT NULL CHECK (presentation IN ('draft', 'find_online')),
    search_tab     TEXT NOT NULL CHECK (search_tab IN ('general', 'catalog_number', 'barcode')),
    search_artist  TEXT NOT NULL,
    search_album   TEXT NOT NULL,
    search_catalog TEXT NOT NULL,
    search_barcode TEXT NOT NULL,
    error          TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- Why a candidate's import failed.
CREATE TABLE IF NOT EXISTS import_candidate_failure (
    content_hash TEXT PRIMARY KEY,
    error        TEXT NOT NULL,
    failed_at    TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The failure where one incoming artist name matches two library artists, each
-- already linked to a different provider.
CREATE TABLE IF NOT EXISTS import_candidate_artist_identity_conflict (
    content_hash                  TEXT PRIMARY KEY,
    incoming_artist_name          TEXT NOT NULL,
    discogs_artist_id             TEXT NOT NULL,
    musicbrainz_artist_id         TEXT NOT NULL,
    discogs_library_artist_id     TEXT NOT NULL,
    musicbrainz_library_artist_id TEXT NOT NULL,
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_failure (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (discogs_library_artist_id) REFERENCES artists (id) ON DELETE RESTRICT,
    FOREIGN KEY (musicbrainz_library_artist_id) REFERENCES artists (id) ON DELETE RESTRICT
) STRICT;

-- ── Identification ────────────────────────────────────────────────────────────

-- What extraction read off a candidate: the disc's table of contents, and
-- whether the barcode and text passes settled or failed.
CREATE TABLE IF NOT EXISTS import_candidate_signals (
    content_hash           TEXT PRIMARY KEY,
    disc_id_state          TEXT NOT NULL CHECK (disc_id_state IN ('computed', 'absent', 'failed')),
    disc_id                TEXT,
    -- The candidate-relative path of the LOG or CUE the disc ID came from, so a
    -- surface can put it on that file's row. NULL for a re-identify pass over a
    -- library release, which derives the ID from stored tracks rather than a
    -- file of a scanned folder.
    disc_id_source_file    TEXT,
    track_count            INTEGER NOT NULL CHECK (track_count >= 0),
    disc_id_failure        TEXT CHECK (disc_id_failure IS NULL OR disc_id_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    disc_id_failure_status INTEGER,
    disc_id_failure_detail TEXT,
    barcode_state          TEXT NOT NULL CHECK (barcode_state IN ('settled', 'failed', 'absent')),
    barcode_failure        TEXT CHECK (barcode_failure IS NULL OR barcode_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    barcode_failure_status INTEGER,
    barcode_failure_detail TEXT,
    text_state             TEXT NOT NULL CHECK (text_state IN ('settled', 'failed')),
    text_failure           TEXT CHECK (text_failure IS NULL OR text_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    text_failure_status    INTEGER,
    text_failure_detail    TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((disc_id_state = 'computed') = (disc_id IS NOT NULL)),
    -- A source file with no computed ID behind it is not a provenance.
    CHECK (disc_id_source_file IS NULL OR disc_id_state = 'computed'),
    CHECK ((disc_id_state = 'failed') = (disc_id_failure IS NOT NULL)),
    CHECK ((barcode_state = 'failed') = (barcode_failure IS NOT NULL)),
    CHECK ((text_state = 'failed') = (text_failure IS NOT NULL)),
    CHECK (disc_id_failure_status IS NULL OR disc_id_failure = 'provider'),
    CHECK ((disc_id_failure = 'diagnostic') = (disc_id_failure_detail IS NOT NULL)),
    CHECK (barcode_failure_status IS NULL OR barcode_failure = 'provider'),
    CHECK ((barcode_failure = 'diagnostic') = (barcode_failure_detail IS NOT NULL)),
    CHECK (text_failure_status IS NULL OR text_failure = 'provider'),
    CHECK ((text_failure = 'diagnostic') = (text_failure_detail IS NOT NULL))
) STRICT;

-- The barcodes and catalog numbers read off a candidate, in reading order,
-- each with the surface it was read from.
CREATE TABLE IF NOT EXISTS import_candidate_signal_value (
    content_hash TEXT NOT NULL,
    list         TEXT NOT NULL CHECK (list IN ('barcode', 'catalog', 'free_text')),
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT NOT NULL,
    origin       TEXT CHECK (origin IS NULL OR origin IN ('disc_toc', 'cue_sheet', 'artwork', 'folder_name', 'filename', 'text_file')),
    -- The candidate-relative path of the file the value was read off, where the
    -- origin is a file: the image OCR found a barcode on, the sheet a field came
    -- from. NULL where the origin names no file (the folder's own name), and for
    -- a re-identify pass over a library release, whose images are stored blobs.
    origin_path  TEXT,
    -- The box the detector drew around the value, as fractions of the image.
    region_x REAL,
    region_y REAL,
    region_width REAL,
    region_height REAL,
    PRIMARY KEY (content_hash, list, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_signals (content_hash) ON DELETE CASCADE,
    CHECK ((list = 'free_text') = (origin IS NULL)),
    -- A file with no origin behind it is not a provenance.
    CHECK (origin_path IS NULL OR origin IS NOT NULL)
) STRICT;

-- Every line of text read off a candidate's surfaces, which ranking reads the
-- folder's own claims out of.
CREATE TABLE IF NOT EXISTS import_candidate_text_line (
    content_hash  TEXT NOT NULL,
    position      INTEGER NOT NULL CHECK (position >= 0),
    text          TEXT NOT NULL,
    origin        TEXT NOT NULL
        CHECK (origin IN ('disc_toc', 'cue_sheet', 'artwork', 'folder_name', 'filename', 'text_file')),
    -- The candidate-relative path of the file the line was read off. NULL for
    -- the folder's own name, and for a re-identify pass over a library release,
    -- whose images are stored blobs rather than files of a folder.
    origin_path   TEXT,
    -- Where on the image the line was read, as fractions of its width and
    -- height with the origin at the top-left corner. All four present or all
    -- four absent; only an artwork line whose recognizer reports positions has
    -- them.
    region_x      REAL,
    region_y      REAL,
    region_width  REAL,
    region_height REAL,
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_signals (content_hash) ON DELETE CASCADE,
    CHECK ((region_x IS NULL) = (region_y IS NULL)
       AND (region_x IS NULL) = (region_width IS NULL)
       AND (region_x IS NULL) = (region_height IS NULL))
) STRICT;

-- Which of the names read off a candidate the user let the lookups use.
CREATE TABLE IF NOT EXISTS import_candidate_lookup_choices (
    content_hash     TEXT PRIMARY KEY,
    disc_id_excluded INTEGER NOT NULL CHECK (disc_id_excluded IN (0, 1)),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The catalog numbers the user chose to look up, in the order chosen.
CREATE TABLE IF NOT EXISTS import_candidate_chosen_catalog (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    value        TEXT NOT NULL,
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_lookup_choices (content_hash) ON DELETE CASCADE
) STRICT;

-- The catalog numbers the user ruled out.
CREATE TABLE IF NOT EXISTS import_candidate_discounted_catalog (
    content_hash TEXT NOT NULL,
    value        TEXT NOT NULL,
    PRIMARY KEY (content_hash, value),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_lookup_choices (content_hash) ON DELETE CASCADE
) STRICT;

-- The barcodes the user ruled out.
CREATE TABLE IF NOT EXISTS import_candidate_excluded_barcode (
    content_hash TEXT NOT NULL,
    value        TEXT NOT NULL,
    PRIMARY KEY (content_hash, value),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_lookup_choices (content_hash) ON DELETE CASCADE
) STRICT;

-- What an identify run concluded, and the ledger it recorded as it ended.
CREATE TABLE IF NOT EXISTS import_candidate_verdict (
    content_hash             TEXT PRIMARY KEY,
    kind                     TEXT NOT NULL
        CHECK (kind IN ('found', 'not_found', 'manual_only', 'failed')),
    -- The tracks the folder played when the verdict was reached. Only a verdict
    -- that found nothing anywhere counts none.
    track_count              INTEGER CHECK (track_count IS NULL OR track_count >= 0),
    -- The typed lookup failures of a failed verdict, serialized as one value
    -- because no query dispatches on their internals; queue placement needs only
    -- the verdict's kind.
    failures_json            TEXT CHECK (
        failures_json IS NULL
        OR (json_valid(failures_json)
            AND json_type(failures_json) = 'array'
            AND json_array_length(failures_json) > 0)
    ),
    -- The ledger the run recorded as it ended, stored whole: no query reads into
    -- it. NULL is "no ledger recorded".
    ledger_json              TEXT CHECK (
        ledger_json IS NULL
        OR (json_valid(ledger_json) AND json_type(ledger_json) = 'object')
    ),
    probed_total_duration_ms INTEGER NOT NULL CHECK (probed_total_duration_ms >= 0),
    identified_at            TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((kind = 'not_found') = (track_count IS NULL)),
    CHECK ((kind = 'failed') = (failures_json IS NOT NULL))
) STRICT;

-- Every release a run's lookups returned, in the order it listed them, with
-- what the record said and which lookup found it.
CREATE TABLE IF NOT EXISTS import_candidate_match (
    content_hash           TEXT NOT NULL,
    position               INTEGER NOT NULL CHECK (position >= 0),
    -- The pressing row this release belongs to, numbered from zero within its
    -- own list: the matches number their rows and the narrowed-out releases
    -- number theirs, each in the order the run listed them.
    pressing               INTEGER NOT NULL CHECK (pressing >= 0),
    source                 TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id             TEXT NOT NULL,
    title                  TEXT NOT NULL,
    artist                 TEXT,
    year                   INTEGER,
    format                 TEXT,
    label                  TEXT,
    catalog_number         TEXT,
    country                TEXT,
    -- What the record said the pressing is made of. 'undescribed': the
    -- response described no media, and there are no medium rows.
    -- 'per_medium': one medium row per medium the record listed, its format
    -- NULL where the record stated none. 'descriptors': one medium row per
    -- format name or qualifier, each stating its text; which medium each
    -- describes is not said.
    media_kind             TEXT NOT NULL
        CHECK (media_kind IN ('undescribed', 'per_medium', 'descriptors')),
    cover_url              TEXT,
    cover_thumbnail_url    TEXT,
    cover_label            TEXT,
    cover_source           TEXT CHECK (cover_source IS NULL OR cover_source IN ('musicbrainz', 'discogs')),
    source_group_id        TEXT,
    -- NULL: nobody asked the source for its tracklist yet. 'listed' /
    -- 'nothing': asked. The total is NULL when any listed track has no length.
    source_tracks_kind     TEXT CHECK (source_tracks_kind IS NULL OR source_tracks_kind IN ('listed', 'nothing')),
    source_tracks_count    INTEGER CHECK (source_tracks_count IS NULL OR source_tracks_count >= 0),
    source_tracks_total_ms INTEGER CHECK (source_tracks_total_ms IS NULL OR source_tracks_total_ms >= 0),
    -- Which lookup returned this release. What the folder's own text says about
    -- it is not here: that is read out of the text lines every time the
    -- verdict is read, so changing what the text is taken to state re-ranks the
    -- rows without re-running anything.
    by_disc_id             INTEGER NOT NULL CHECK (by_disc_id IN (0, 1)),
    by_barcode             INTEGER NOT NULL CHECK (by_barcode IN (0, 1)),
    by_catalog             INTEGER NOT NULL CHECK (by_catalog IN (0, 1)),
    -- The title search the run falls back on when no identifier named
    -- anything. Never set beside the three above: the search is asked only
    -- once they have all come back empty.
    by_search              INTEGER NOT NULL CHECK (by_search IN (0, 1)),
    narrowed_out           INTEGER NOT NULL DEFAULT 0 CHECK (narrowed_out IN (0, 1)),
    PRIMARY KEY (content_hash, position),
    -- The medium rows reference the match together with its media kind, so a
    -- row can only ever belong to a match of the kind it was written for.
    UNIQUE (content_hash, position, media_kind),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_verdict (content_hash) ON DELETE CASCADE,
    CHECK ((cover_url IS NULL) = (cover_thumbnail_url IS NULL) AND (cover_url IS NULL) = (cover_label IS NULL) AND (cover_url IS NULL) = (cover_source IS NULL)),
    CHECK ((source_tracks_kind = 'listed') = (source_tracks_count IS NOT NULL)),
    CHECK (source_tracks_total_ms IS NULL OR source_tracks_kind = 'listed')
) STRICT;

-- Every barcode a matched record states.
CREATE TABLE IF NOT EXISTS import_candidate_match_barcode (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    barcode      TEXT NOT NULL CHECK (barcode <> ''),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match (content_hash, position) ON DELETE CASCADE
) STRICT;

-- Every other catalog entry a matched record links to.
CREATE TABLE IF NOT EXISTS import_candidate_match_link (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    catalog      TEXT NOT NULL CHECK (catalog <> ''),
    key          TEXT NOT NULL CHECK (key <> ''),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match (content_hash, position) ON DELETE CASCADE
) STRICT;

-- What a matched record said its media are, one row per medium or per format
-- descriptor, per the match's media kind.
CREATE TABLE IF NOT EXISTS import_candidate_match_medium (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    media_kind   TEXT NOT NULL CHECK (media_kind IN ('per_medium', 'descriptors')),
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    format       TEXT CHECK (format IS NULL OR format <> ''),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position, media_kind)
        REFERENCES import_candidate_match (content_hash, position, media_kind)
        ON DELETE CASCADE,
    CHECK (media_kind = 'per_medium' OR format IS NOT NULL)
) STRICT;

-- ── Catalog documents ─────────────────────────────────────────────────────────

-- The provider responses a lookup fetched, kept as returned so a draft can be
-- re-read without asking again.
CREATE TABLE IF NOT EXISTS source_release_payloads (
    -- Which lookup produced this document, and therefore what
    -- `source_release_id` names:
    --   'musicbrainz'                     the release itself
    --   'musicbrainz_release_group'       its release group, by group id
    --   'discogs'                         a Discogs release
    --   'discogs_master'                  a Discogs master, by master id
    --   'musicbrainz_discogs_xref'        the MusicBrainz release cross-linked
    --                                     to a Discogs one, by the *Discogs*
    --                                     release id — MusicBrainz's URL lookup
    --                                     found it, so nothing in the Discogs
    --                                     document names it back
    --   'wikidata'                        the Wikidata item a MusicBrainz
    --                                     release or release group links to,
    --                                     by item id
    source TEXT NOT NULL,
    source_release_id TEXT NOT NULL,
    -- The document as the provider returned it.
    json TEXT NOT NULL CHECK (json_valid(json)),
    fetched_at TEXT NOT NULL,
    PRIMARY KEY (source, source_release_id)
) STRICT;
