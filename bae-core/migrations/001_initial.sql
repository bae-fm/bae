-- bae's application schema. coven runs this (idempotently) after its own
-- bookkeeping migration when it opens the connection it owns, so every
-- `CREATE TABLE`/`CREATE INDEX` is `IF NOT EXISTS`: re-running over a
-- snapshot-bootstrapped database that already carries the schema is a no-op.
--
-- coven's own bookkeeping tables (sync cursors, the cloud outbox, the circle
-- and store-write ledgers) are created by coven's MIGRATION_SQL, not here.
--
-- Sections: the library, playback, watched folders and their scans, import
-- candidates, identification, and the catalog releases lookups fetched.

-- ── The library ───────────────────────────────────────────────────────────────

-- Every artist the library knows, whether credited on a release, a track, or a
-- work. The provider ids are what a later lookup matches an incoming artist to
-- first; `name_key` is what it matches by when no id does — the name folded by
-- `util::text::normalize` (case, diacritics and spacing dropped), written in
-- the same statement as `name`.
CREATE TABLE IF NOT EXISTS artists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_key TEXT NOT NULL,
    sort_name TEXT,
    discogs_artist_id TEXT,
    musicbrainz_artist_id TEXT,

    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artists_name ON artists (name COLLATE NOCASE);

CREATE INDEX IF NOT EXISTS idx_artists_name_key ON artists (name_key);

CREATE INDEX IF NOT EXISTS idx_artists_discogs_id ON artists (discogs_artist_id);

CREATE INDEX IF NOT EXISTS idx_artists_mb_id ON artists (musicbrainz_artist_id);

-- Two library artists the user confirmed are one: `id` is the absorbed artist,
-- `into_artist_id` the one it became. Both rows stay, so a credit another
-- device gave the absorbed artist while apart still has its parent; every read
-- shows an artist through `merged_artist_survivors`.
CREATE TABLE IF NOT EXISTS artist_merges (
    id TEXT PRIMARY KEY,
    into_artist_id TEXT NOT NULL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    CHECK (id <> into_artist_id),
    FOREIGN KEY (id) REFERENCES artists (id) ON DELETE CASCADE,
    FOREIGN KEY (into_artist_id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artist_merges_into ON artist_merges (into_artist_id);

-- Every merged artist and the artist it now shows as: the end of its chain of
-- merges. Devices that merged one pair in opposite directions while apart leave
-- a cycle; it shows as the smallest id in it, the same on every device. An
-- artist absent here shows as itself.
CREATE VIEW IF NOT EXISTS merged_artist_survivors AS
WITH RECURSIVE hop(start, current, depth) AS (
    SELECT id, into_artist_id, 1 FROM artist_merges
    UNION ALL
    SELECT hop.start, merge.into_artist_id, hop.depth + 1
    FROM hop JOIN artist_merges merge ON merge.id = hop.current
    WHERE hop.depth < 64
),
resolved(artist_id, survivor_id) AS (
    SELECT start,
           COALESCE(
               MIN(CASE WHEN current NOT IN (SELECT id FROM artist_merges) THEN current END),
               MIN(current)
           )
    FROM hop
    GROUP BY start
)
SELECT artist_id, survivor_id FROM resolved WHERE artist_id <> survivor_id;

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
    -- The catalog of the record (in `release_records`) the stored metadata was
    -- read from, or NULL. One value per release, so two devices choosing
    -- different records while apart merge to one of them.
    draft_catalog TEXT,
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
-- the pressing itself or the album it belongs to. The one the stored metadata
-- was read from is `releases.draft_catalog`.
CREATE TABLE IF NOT EXISTS release_records (
    id          TEXT NOT NULL PRIMARY KEY,
    release_id  TEXT NOT NULL,
    catalog     TEXT NOT NULL,
    key         TEXT NOT NULL,
    album_key   TEXT,
    url         TEXT NOT NULL CHECK (url <> ''),
    _updated_at TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('pressing', 'album')),
    CHECK (kind = 'pressing' OR album_key IS NULL),
    UNIQUE (release_id, catalog),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_records_catalog_key ON release_records (catalog, key) WHERE kind = 'pressing';

CREATE INDEX IF NOT EXISTS idx_release_records_catalog_album ON release_records
    (catalog, CASE WHEN kind = 'album' THEN key ELSE album_key END);

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

-- Folders read as one release, or a folder whose releases are kept apart.
--
-- A grouping anchored at a folder reads every release below that folder as
-- one (`combined = 1`) or keeps them apart (`combined = 0`); the scan proposes
-- one wherever a folder yields several releases and nothing is stored, and the
-- person's answer replaces it. A grouping with no anchor takes in the
-- releases its members name, from anywhere; it is always combined, and it
-- takes those releases out of the queue for as long as it stands.
CREATE TABLE IF NOT EXISTS release_grouping (
    -- The candidate key of the release the grouping reads as one.
    key                  TEXT PRIMARY KEY,
    -- The watched folder its release is listed under.
    watched_folder_path  TEXT NOT NULL,
    anchor_relative_path TEXT,
    combined             INTEGER NOT NULL CHECK (combined IN (0, 1)),
    -- Who decided. The scan reads a folder its own way when nothing is stored
    -- and records that as 'heuristic'; the user's own answer replaces it as
    -- 'user' and is never read over again.
    author               TEXT NOT NULL CHECK (author IN ('user', 'heuristic')),
    skipped              INTEGER NOT NULL DEFAULT 0 CHECK (skipped IN (0, 1)),
    -- Why the release a grouping with no anchor reads cannot be worked on as
    -- it stands, typed so every surface says it in the person's language: a
    -- release it takes in changed or is gone, the files of the folder it sits
    -- in go with another release or are still downloading, or its releases
    -- make no release. The release keeps what it was last built from until
    -- that is fixed or the grouping is undone. `blocked_subject` names the
    -- folder — or, for 'unbuildable', the diagnostic — and `blocked_holder`
    -- the release already reading the folder's files; both are for the log.
    blocked              TEXT CHECK (blocked IS NULL OR blocked IN (
        'source_changed', 'source_gone', 'folder_files_taken',
        'folder_files_contested', 'folder_files_downloading', 'unbuildable'
    )),
    blocked_subject      TEXT,
    blocked_holder       TEXT,
    -- For a grouping with no anchor: the folder every release it takes in
    -- sits directly in, whose sidecar files (scan_sidecar) are the release's
    -- own; and whether its release reads them. A folder's files go with one
    -- release at most: while several groupings sit in a folder that has
    -- files, a new one is refused, and on a rebuild each one but the reader
    -- is blocked with an error.
    parent_folder        TEXT,
    reads_parent_files   INTEGER NOT NULL DEFAULT 0 CHECK (reads_parent_files IN (0, 1)),
    UNIQUE (watched_folder_path, anchor_relative_path),
    CHECK (anchor_relative_path IS NOT NULL OR (combined = 1 AND author = 'user')),
    CHECK (anchor_relative_path IS NULL OR blocked IS NULL),
    CHECK ((blocked IS NULL) = (blocked_subject IS NULL)),
    CHECK ((blocked IS 'folder_files_taken') = (blocked_holder IS NOT NULL)),
    CHECK (anchor_relative_path IS NULL OR parent_folder IS NULL),
    CHECK (reads_parent_files = 0 OR parent_folder IS NOT NULL),
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path)
        ON DELETE CASCADE
) STRICT;

-- The groupings sitting in a folder, and the one that reads its files.
CREATE INDEX IF NOT EXISTS release_grouping_by_parent ON release_grouping (parent_folder);

CREATE UNIQUE INDEX IF NOT EXISTS release_grouping_parent_reader
    ON release_grouping (parent_folder) WHERE reads_parent_files = 1;

-- The releases a grouping with no anchor takes in, in play order.
CREATE TABLE IF NOT EXISTS release_grouping_member (
    grouping_key        TEXT NOT NULL
        REFERENCES release_grouping (key) ON DELETE CASCADE,
    position            INTEGER NOT NULL CHECK (position >= 0),
    member_key          TEXT NOT NULL UNIQUE,
    watched_folder_path TEXT NOT NULL,
    PRIMARY KEY (grouping_key, position)
) STRICT;

CREATE INDEX IF NOT EXISTS release_grouping_member_by_root
    ON release_grouping_member (watched_folder_path);

-- A grouping outlives no watched folder it takes a release from.
CREATE TRIGGER IF NOT EXISTS remove_root_groupings BEFORE DELETE ON watched_import_folders
BEGIN
    DELETE FROM release_grouping
    WHERE key IN (
        SELECT grouping_key FROM release_grouping_member
        WHERE watched_folder_path = OLD.path
    );
END;

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
    -- The volume the folder was on when this scan began: 'local' or 'network'.
    -- Asked of the system once per scan, so reading where scans stand never
    -- waits on a mount.
    volume              TEXT NOT NULL CHECK (volume IN ('local', 'network')),
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

-- One release the scan found: a folder, or folders a grouping reads as one.
-- `path` is the release's key — its folder's path, or its grouping's key.
CREATE TABLE IF NOT EXISTS scan_candidate (
    watched_folder_path            TEXT NOT NULL,
    path                           TEXT NOT NULL,
    generation                     INTEGER NOT NULL CHECK (generation >= 0),
    kind                           TEXT NOT NULL CHECK (kind IN ('tentative', 'valid', 'invalid')),
    name                           TEXT NOT NULL,
    display_path                   TEXT NOT NULL,
    -- The folder the release is shown as.
    folder                         TEXT NOT NULL,
    -- The folder its files are read from, and whether all of them below it.
    file_root                      TEXT NOT NULL,
    scope                          TEXT NOT NULL CHECK (scope IN ('direct', 'recursive')),
    content_hash                   TEXT,
    file_edit_revision             INTEGER NOT NULL DEFAULT 0 CHECK (file_edit_revision >= 0),
    grouping_key                   TEXT CHECK (grouping_key IS NULL OR grouping_key = path),
    invalid_reason                 TEXT CHECK (invalid_reason IS NULL OR invalid_reason IN ('corrupt_audio', 'corrupt_image', 'no_valid_audio')),
    invalid_reason_path            TEXT,
    first_seen_at                  INTEGER,
    source_date                    INTEGER,
    source_date_kind               TEXT CHECK ((source_date IS NULL AND source_date_kind IS NULL)
        OR (source_date IS NOT NULL AND source_date_kind IS NOT NULL
            AND source_date_kind IN ('added_to_directory', 'created'))),
    -- 'folder' for what a scan read; 'grouping' for a release a grouping with
    -- no anchor builds from the releases it takes in.
    source_kind                    TEXT NOT NULL DEFAULT 'folder' CHECK (source_kind IN ('folder', 'grouping')),
    PRIMARY KEY (watched_folder_path, path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE,
    CHECK ((kind = 'invalid') = (invalid_reason IS NOT NULL)),
    CHECK ((kind = 'invalid') = (content_hash IS NULL)),
    CHECK ((source_kind = 'grouping') <= (grouping_key IS NOT NULL)),
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

-- Which release holds a file on disk: what a sidecar holding the same file
-- replaces.
CREATE INDEX IF NOT EXISTS idx_scan_candidate_file_absolute
    ON scan_candidate_file (absolute_path);

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

-- The folders a release read from several is made of, in play order, and the
-- prefix each one's files take in the release.
CREATE TABLE IF NOT EXISTS scan_candidate_part (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    folder              TEXT NOT NULL,
    prefix              TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, position),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE
) STRICT;

-- The files under a folder that no release the scan read there owns: a cover
-- or a booklet beside disc folders kept as releases of their own. Stored by
-- the scan like its candidates and pruned with them. A folder's files are
-- stored once: a sidecar and a scanned release holding the same file replace
-- one another, whichever is written later. Watched roots never overlap, so
-- the folder alone names it.
CREATE TABLE IF NOT EXISTS scan_sidecar (
    folder              TEXT PRIMARY KEY,
    watched_folder_path TEXT NOT NULL,
    generation          INTEGER NOT NULL CHECK (generation >= 0),
    -- 'valid' with its files; 'invalid' when one of them is broken, so a
    -- release taking them in cannot be imported; 'downloading' while a
    -- download into the folder runs, so what it holds is not known yet.
    state               TEXT NOT NULL CHECK (state IN ('valid', 'invalid', 'downloading')),
    invalid_reason      TEXT CHECK (invalid_reason IS NULL OR invalid_reason IN ('corrupt_audio', 'corrupt_image')),
    invalid_reason_path TEXT,
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE,
    CHECK ((state = 'invalid') = (invalid_reason IS NOT NULL)),
    CHECK ((invalid_reason IS NULL) = (invalid_reason_path IS NULL))
) STRICT;

CREATE INDEX IF NOT EXISTS idx_scan_sidecar_root ON scan_sidecar (watched_folder_path);

-- One file of a valid sidecar, in release file order, with its role.
CREATE TABLE IF NOT EXISTS scan_sidecar_file (
    folder              TEXT NOT NULL REFERENCES scan_sidecar (folder) ON DELETE CASCADE,
    position            INTEGER NOT NULL CHECK (position >= 0),
    relative_path       TEXT NOT NULL,
    absolute_path       TEXT NOT NULL UNIQUE,
    size                INTEGER NOT NULL CHECK (size >= 0),
    modified_at_ns      INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    role                TEXT NOT NULL CHECK (role IN ('artwork', 'document', 'other')),
    PRIMARY KEY (folder, position),
    UNIQUE (folder, relative_path)
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

-- A release a grouping reads as one leaves the queue with its grouping.
CREATE TRIGGER IF NOT EXISTS remove_grouping_candidate AFTER DELETE ON release_grouping
BEGIN
    DELETE FROM scan_candidate WHERE path = OLD.key;
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
    -- Who wrote the draft: nobody (the blank one discovery creates), discovery
    -- seeding it from the folder's tags, an identification run applying its
    -- pick, or a person. Which provenance each may carry is checked where the
    -- draft is saved: it spans this row and the provenance row.
    author         TEXT NOT NULL
        CHECK (author IN ('nobody', 'prefill', 'identification', 'person')),
    -- What the import list places and shows a row by, written with the draft
    -- from the draft so the list reads two columns instead of every draft
    -- whole: whether it is blank, and whether it is a complete, valid edit.
    draft_blank    INTEGER NOT NULL CHECK (draft_blank IN (0, 1)),
    draft_valid    INTEGER NOT NULL CHECK (draft_valid IN (0, 1)),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The album artists of the draft: either a library artist the person picked
-- ('picked', by id), or a credit ('credit') — what a source or the person
-- said, with no claim about the library. Which library artist a credit is, if
-- any, is decided each time the draft is read and inside the import's write.
CREATE TABLE IF NOT EXISTS import_candidate_album_artist_assignment (
    content_hash          TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('picked', 'credit')),
    artist_id             TEXT,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE RESTRICT,
    CHECK (
        (assignment_kind = 'picked' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL)
        OR
        (assignment_kind = 'credit' AND artist_id IS NULL AND name IS NOT NULL AND name <> '')
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

-- The per-track artists of the draft, where a track does not take the album's;
-- 'picked' and 'credit' as for the album's.
CREATE TABLE IF NOT EXISTS import_candidate_track_artist_assignment (
    content_hash          TEXT NOT NULL,
    track_id              TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('picked', 'credit')),
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
        (assignment_kind = 'picked' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL)
        OR
        (assignment_kind = 'credit' AND artist_id IS NULL AND name IS NOT NULL AND name <> '')
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
    CHECK ((kind = 'remote') = (url IS NOT NULL AND source IS NOT NULL)),
    -- What the copies reference: only a remote choice has an address.
    UNIQUE (content_hash, url)
) STRICT;

-- The downscaled copies the catalog serves of a remote cover choice, one per
-- box size: a copy's longer side is at most max_edge pixels. Each names the
-- image it is a copy of, so a choice with no address — a folder file, an
-- embedded image — cannot have any.
CREATE TABLE IF NOT EXISTS import_candidate_cover_copy (
    content_hash TEXT NOT NULL,
    image_url    TEXT NOT NULL,
    max_edge     INTEGER NOT NULL CHECK (max_edge > 0),
    url          TEXT NOT NULL CHECK (url <> ''),
    PRIMARY KEY (content_hash, max_edge),
    FOREIGN KEY (content_hash, image_url)
        REFERENCES import_candidate_cover (content_hash, url) ON DELETE CASCADE
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

-- Where the draft was read from. Who wrote it is the draft row's `author`. A
-- release it names is one bae fetched and stored.
CREATE TABLE IF NOT EXISTS import_candidate_draft_provenance (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('external_release', 'file_tags')),
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    release_id   TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (source, release_id) REFERENCES source_release (catalog, release_id),
    -- Only a release names a release; File Tags names the candidate's own files.
    CHECK ((kind = 'external_release') = (source IS NOT NULL)),
    CHECK ((kind = 'external_release') = (release_id IS NOT NULL))
) STRICT;

-- The releases in the other catalogs that the draft's own record links to.
CREATE TABLE IF NOT EXISTS import_candidate_provenance_partner (
    content_hash TEXT NOT NULL,
    source       TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id   TEXT NOT NULL,
    PRIMARY KEY (content_hash, source),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_draft_provenance (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (source, release_id) REFERENCES source_release (catalog, release_id)
) STRICT;

-- The draft was read from the releases its provenance names: a row here says
-- the draft's source tracks index those releases' tracklists as laid out
-- against the lengths below.
CREATE TABLE IF NOT EXISTS import_candidate_applied_source (
    content_hash TEXT PRIMARY KEY,
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_draft_provenance (content_hash) ON DELETE CASCADE
) STRICT;

-- The measured lengths of the draft's tracks, in order, when it was read.
CREATE TABLE IF NOT EXISTS import_candidate_applied_length (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL CHECK (position >= 0),
    duration_ms  INTEGER NOT NULL CHECK (duration_ms >= 0),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_applied_source (content_hash) ON DELETE CASCADE
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

-- Which of the names read off a candidate the user let the lookups use, and
-- the words the user typed for the title search in place of the draft's own
-- (both absent when the draft's title is searched).
CREATE TABLE IF NOT EXISTS import_candidate_lookup_choices (
    content_hash     TEXT PRIMARY KEY,
    disc_id_excluded INTEGER NOT NULL CHECK (disc_id_excluded IN (0, 1)),
    search_album     TEXT,
    search_artist    TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((search_album IS NULL) = (search_artist IS NULL))
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
    content_hash  TEXT PRIMARY KEY,
    kind          TEXT NOT NULL
        CHECK (kind IN ('found', 'not_found', 'manual_only', 'failed')),
    -- The tracks the folder played when the verdict was reached. Only a verdict
    -- that found nothing anywhere counts none.
    track_count   INTEGER CHECK (track_count IS NULL OR track_count >= 0),
    -- The typed lookup failures of a failed verdict, serialized as one value
    -- because no query dispatches on their internals; queue placement needs only
    -- the verdict's kind.
    failures_json TEXT CHECK (
        failures_json IS NULL
        OR (json_valid(failures_json)
            AND json_type(failures_json) = 'array'
            AND json_array_length(failures_json) > 0)
    ),
    -- The ledger the run recorded as it ended, stored whole: no query reads into
    -- it. NULL is "no ledger recorded".
    ledger_json   TEXT CHECK (
        ledger_json IS NULL
        OR (json_valid(ledger_json) AND json_type(ledger_json) = 'object')
    ),
    identified_at TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((kind = 'not_found') = (track_count IS NULL)),
    CHECK ((kind = 'failed') = (failures_json IS NOT NULL))
) STRICT;

-- Every release a run's lookups returned, in the order it listed them, with
-- what the record said and which lookup found it.
CREATE TABLE IF NOT EXISTS import_candidate_match (
    content_hash        TEXT NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    -- The pressing row this release belongs to, numbered from zero within its
    -- own list: the matches number their rows and the narrowed-out releases
    -- number theirs, each in the order the run listed them.
    pressing            INTEGER NOT NULL CHECK (pressing >= 0),
    source              TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id          TEXT NOT NULL,
    title               TEXT NOT NULL,
    artist              TEXT,
    year                INTEGER,
    format              TEXT,
    label               TEXT,
    catalog_number      TEXT,
    country             TEXT,
    -- What the record said the pressing is made of. 'undescribed': the
    -- response described no media, and there are no medium rows.
    -- 'per_medium': one medium row per medium the record listed, its format
    -- NULL where the record stated none. 'descriptors': one medium row per
    -- format name or qualifier, each stating its text; which medium each
    -- describes is not said.
    media_kind          TEXT NOT NULL
        CHECK (media_kind IN ('undescribed', 'per_medium', 'descriptors')),
    -- The lead cover's original; its downscaled copies are
    -- import_candidate_match_cover_copy rows.
    cover_url           TEXT,
    cover_label         TEXT,
    cover_source        TEXT CHECK (cover_source IS NULL OR cover_source IN ('musicbrainz', 'discogs')),
    -- 'stated': the record's catalog says the image is there. 'unstated': an
    -- address the record said nothing about. No cover: the record states none.
    cover_standing      TEXT CHECK (cover_standing IS NULL OR cover_standing IN ('stated', 'unstated')),
    source_group_id     TEXT,
    -- What the record's catalog says its album is on the other lookup catalog.
    -- 'not_asked': never read. 'read': read, and the album link rows name
    -- what the statements named. 'unread': a document the reading needed
    -- could not be had, and no statement named an album.
    album_links         TEXT NOT NULL CHECK (album_links IN ('not_asked', 'read', 'unread')),
    -- NULL: nobody asked the source for its tracklist yet. 'listed' /
    -- 'nothing': asked.
    source_tracks_kind  TEXT CHECK (source_tracks_kind IS NULL OR source_tracks_kind IN ('listed', 'nothing')),
    source_tracks_count INTEGER CHECK (source_tracks_count IS NULL OR source_tracks_count >= 0),
    -- Which lookup returned this release. What the folder's own text says about
    -- it is not here: that is read out of the text lines every time the
    -- verdict is read, so changing what the text is taken to state re-ranks the
    -- rows without re-running anything.
    by_disc_id          INTEGER NOT NULL CHECK (by_disc_id IN (0, 1)),
    by_barcode          INTEGER NOT NULL CHECK (by_barcode IN (0, 1)),
    by_catalog          INTEGER NOT NULL CHECK (by_catalog IN (0, 1)),
    -- The title search the run falls back on when no identifier named
    -- anything. Never set beside the three above: the search is asked only
    -- once they have all come back empty.
    by_search           INTEGER NOT NULL CHECK (by_search IN (0, 1)),
    -- Returned by no lookup: the release whose own document names this one as
    -- the same release, which the run read it through to learn its album.
    -- NULL for a release a lookup returned or a person chose.
    named_by_catalog    TEXT CHECK (named_by_catalog IS NULL OR named_by_catalog <> ''),
    named_by_key        TEXT CHECK (named_by_key IS NULL OR named_by_key <> ''),
    narrowed_out        INTEGER NOT NULL DEFAULT 0 CHECK (narrowed_out IN (0, 1)),
    PRIMARY KEY (content_hash, position),
    -- The medium rows reference the match together with its media kind, so a
    -- row can only ever belong to a match of the kind it was written for.
    UNIQUE (content_hash, position, media_kind),
    -- What the cover copies reference: only a match with a cover has one.
    UNIQUE (content_hash, position, cover_url),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_verdict (content_hash) ON DELETE CASCADE,
    CHECK ((cover_url IS NULL) = (cover_label IS NULL) AND (cover_url IS NULL) = (cover_source IS NULL) AND (cover_url IS NULL) = (cover_standing IS NULL)),
    CHECK ((source_tracks_kind = 'listed') = (source_tracks_count IS NOT NULL)),
    CHECK ((named_by_catalog IS NULL) = (named_by_key IS NULL)),
    CHECK (named_by_catalog IS NULL
           OR (by_disc_id = 0 AND by_barcode = 0 AND by_catalog = 0 AND by_search = 0))
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

-- The downscaled copies the catalog serves of a match's cover, one per box
-- size: a copy's longer side is at most max_edge pixels. Each names the
-- cover it is a copy of, so a match with no cover cannot have any.
CREATE TABLE IF NOT EXISTS import_candidate_match_cover_copy (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    cover_url    TEXT NOT NULL,
    max_edge     INTEGER NOT NULL CHECK (max_edge > 0),
    url          TEXT NOT NULL CHECK (url <> ''),
    PRIMARY KEY (content_hash, position, max_edge),
    FOREIGN KEY (content_hash, position, cover_url)
        REFERENCES import_candidate_match (content_hash, position, cover_url) ON DELETE CASCADE
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

-- Every other catalog's album a statement names as a matched record's album —
-- the Discogs masters a MusicBrainz release group is — for a match whose
-- album_links is 'read', with the statement that names it. 'page': the
-- group's own page links it. 'wikidata': the Wikidata item the group's page
-- links states it. 'release': musicbrainz_release, one of the group's
-- releases, links the twin release as itself, and the twin's own document
-- files it under the album.
CREATE TABLE IF NOT EXISTS import_candidate_match_album_link (
    content_hash        TEXT NOT NULL,
    position            INTEGER NOT NULL,
    ordinal             INTEGER NOT NULL CHECK (ordinal >= 0),
    catalog             TEXT NOT NULL CHECK (catalog <> ''),
    key                 TEXT NOT NULL CHECK (key <> ''),
    stated              TEXT NOT NULL CHECK (stated IN ('page', 'wikidata', 'release')),
    wikidata_item       TEXT CHECK (wikidata_item IS NULL OR wikidata_item <> ''),
    musicbrainz_release TEXT CHECK (musicbrainz_release IS NULL OR musicbrainz_release <> ''),
    twin_catalog        TEXT CHECK (twin_catalog IS NULL OR twin_catalog <> ''),
    twin_key            TEXT CHECK (twin_key IS NULL OR twin_key <> ''),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match (content_hash, position) ON DELETE CASCADE,
    CHECK ((stated = 'wikidata') = (wikidata_item IS NOT NULL)),
    CHECK ((stated = 'release') = (musicbrainz_release IS NOT NULL)),
    CHECK ((stated = 'release') = (twin_catalog IS NOT NULL)),
    CHECK ((stated = 'release') = (twin_key IS NOT NULL))
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

-- ── Catalog releases ──────────────────────────────────────────────────────────

-- One catalog release bae fetched, with every fact the import reads about it
-- extracted from the provider's documents when it was fetched. Device-local:
-- any device can fetch a release again. Fetching it again replaces every row
-- under it at once.
CREATE TABLE IF NOT EXISTS source_release (
    catalog            TEXT NOT NULL CHECK (catalog IN ('musicbrainz', 'discogs')),
    release_id         TEXT NOT NULL CHECK (release_id <> ''),
    -- The album the release's own catalog files it under: its MusicBrainz
    -- release group or its Discogs master.
    source_group_id    TEXT,
    -- The album's facts: the release's own, and where it states none, what
    -- its cross-referenced release and its album's documents state.
    album_title        TEXT NOT NULL,
    album_year         INTEGER,
    -- The pressing's facts, resolved the same way.
    year               INTEGER,
    format             TEXT,
    label              TEXT,
    catalog_number     TEXT,
    country            TEXT,
    barcode            TEXT,
    -- The MusicBrainz release whose Cover Art Archive gallery the picker
    -- opens: this release, or the one a Discogs release is cross-referenced
    -- to, with its release group.
    archive_release_id TEXT,
    archive_group_id   TEXT,
    fetched_at         TEXT NOT NULL,
    PRIMARY KEY (catalog, release_id),
    CHECK (archive_release_id IS NOT NULL OR archive_group_id IS NULL)
) STRICT;

-- The album's artists, in credit order.
CREATE TABLE IF NOT EXISTS source_release_album_artist (
    catalog               TEXT NOT NULL,
    release_id            TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- The releases on other catalogs a MusicBrainz release names as the same
-- release, in relation order.
CREATE TABLE IF NOT EXISTS source_release_link (
    catalog      TEXT NOT NULL CHECK (catalog = 'musicbrainz'),
    release_id   TEXT NOT NULL,
    position     INTEGER NOT NULL CHECK (position >= 0),
    link_catalog TEXT NOT NULL CHECK (link_catalog <> ''),
    link_key     TEXT NOT NULL CHECK (link_key <> ''),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- A Discogs release's format names and qualifiers: one flat list that does
-- not say which medium each describes.
CREATE TABLE IF NOT EXISTS source_release_format (
    catalog    TEXT NOT NULL CHECK (catalog = 'discogs'),
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    descriptor TEXT NOT NULL,
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- What another catalog says this release is ('pressing', with the album it
-- files that pressing under) or what its album is ('album'). The release's
-- own catalog is the release row itself.
CREATE TABLE IF NOT EXISTS source_release_record (
    catalog        TEXT NOT NULL,
    release_id     TEXT NOT NULL,
    record_catalog TEXT NOT NULL CHECK (record_catalog <> ''),
    kind           TEXT NOT NULL CHECK (kind IN ('pressing', 'album')),
    key            TEXT NOT NULL CHECK (key <> ''),
    album_key      TEXT,
    PRIMARY KEY (catalog, release_id, record_catalog),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE,
    CHECK (record_catalog <> catalog),
    CHECK (kind = 'pressing' OR album_key IS NULL)
) STRICT;

-- What reading a MusicBrainz release group found it to be on another
-- catalog, whichever list read it: each album a statement names, with the
-- statement, in the columns import_candidate_match_album_link uses. A stored
-- release of either album reads the other back as one of its records, so a
-- release whose own documents never reach the join still names it.
-- Device-local, like the releases: a later reading of the group replaces its
-- rows.
CREATE TABLE IF NOT EXISTS release_group_album_link (
    release_group       TEXT NOT NULL CHECK (release_group <> ''),
    catalog             TEXT NOT NULL CHECK (catalog <> '' AND catalog <> 'musicbrainz'),
    key                 TEXT NOT NULL CHECK (key <> ''),
    stated              TEXT NOT NULL CHECK (stated IN ('page', 'wikidata', 'release')),
    wikidata_item       TEXT CHECK (wikidata_item IS NULL OR wikidata_item <> ''),
    musicbrainz_release TEXT CHECK (musicbrainz_release IS NULL OR musicbrainz_release <> ''),
    twin_catalog        TEXT CHECK (twin_catalog IS NULL OR twin_catalog <> ''),
    twin_key            TEXT CHECK (twin_key IS NULL OR twin_key <> ''),
    PRIMARY KEY (release_group, catalog, key),
    CHECK ((stated = 'wikidata') = (wikidata_item IS NOT NULL)),
    CHECK ((stated = 'release') = (musicbrainz_release IS NOT NULL)),
    CHECK ((stated = 'release') = (twin_catalog IS NOT NULL)),
    CHECK ((stated = 'release') = (twin_key IS NOT NULL))
) STRICT;
CREATE INDEX IF NOT EXISTS release_group_album_link_album
    ON release_group_album_link (catalog, key);

-- The images the release offers a picker: its pressing's own ('release'),
-- then its album's ('album'), each in the order they are offered.
CREATE TABLE IF NOT EXISTS source_release_cover (
    catalog       TEXT NOT NULL,
    release_id    TEXT NOT NULL,
    scope         TEXT NOT NULL CHECK (scope IN ('release', 'album')),
    position      INTEGER NOT NULL CHECK (position >= 0),
    -- The original; its downscaled copies are source_release_cover_copy rows.
    url           TEXT NOT NULL,
    label         TEXT NOT NULL,
    source        TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    -- Whether a catalog stated the image is there, or it is an address
    -- nothing said anything about.
    standing      TEXT NOT NULL CHECK (standing IN ('stated', 'unstated')),
    PRIMARY KEY (catalog, release_id, scope, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- The downscaled copies the catalog serves of one offered image, one per box
-- size: a copy's longer side is at most max_edge pixels.
CREATE TABLE IF NOT EXISTS source_release_cover_copy (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    scope      TEXT NOT NULL,
    position   INTEGER NOT NULL,
    max_edge   INTEGER NOT NULL CHECK (max_edge > 0),
    url        TEXT NOT NULL CHECK (url <> ''),
    PRIMARY KEY (catalog, release_id, scope, position, max_edge),
    FOREIGN KEY (catalog, release_id, scope, position)
        REFERENCES source_release_cover (catalog, release_id, scope, position) ON DELETE CASCADE
) STRICT;

-- The MusicBrainz release groups the album was read from, whose Cover Art
-- Archive galleries the picker opens too.
CREATE TABLE IF NOT EXISTS source_release_archive_group (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    group_id   TEXT NOT NULL CHECK (group_id <> ''),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- The composer credits a Discogs release states for itself rather than for
-- one track, at their positions among its credits.
CREATE TABLE IF NOT EXISTS source_release_role (
    catalog               TEXT NOT NULL CHECK (catalog = 'discogs'),
    release_id            TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    role                  TEXT,
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- Every medium of the release, in order. Only MusicBrainz states a medium's
-- format; a Discogs release's mediums are the runs of rows its positions
-- number as one disc.
CREATE TABLE IF NOT EXISTS source_release_medium (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    format     TEXT,
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- One row of a medium's tracklist. `entry` numbers the release's rows in
-- tracklist order, a Discogs index's sub-tracks right after it with the index
-- as their parent. A 'heading' titles the sub-track rows that follow it.
CREATE TABLE IF NOT EXISTS source_release_entry (
    catalog     TEXT NOT NULL,
    release_id  TEXT NOT NULL,
    entry       INTEGER NOT NULL CHECK (entry >= 0),
    medium      INTEGER NOT NULL,
    parent      INTEGER CHECK (parent IS NULL OR parent < entry),
    kind        TEXT NOT NULL CHECK (kind IN ('track', 'heading', 'index')),
    -- The position the catalog prints ('A1', '1-2'); NULL where it prints none.
    position    TEXT CHECK (position IS NULL OR position <> ''),
    -- MusicBrainz's own count of the track within its medium.
    number      INTEGER,
    -- NULL for a MusicBrainz track with no usable title, which reading
    -- refuses.
    title       TEXT,
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    PRIMARY KEY (catalog, release_id, entry),
    FOREIGN KEY (catalog, release_id, medium)
        REFERENCES source_release_medium (catalog, release_id, position) ON DELETE CASCADE,
    FOREIGN KEY (catalog, release_id, parent)
        REFERENCES source_release_entry (catalog, release_id, entry) ON DELETE CASCADE,
    CHECK (kind = 'track' OR catalog = 'discogs'),
    CHECK (parent IS NULL OR catalog = 'discogs')
) STRICT;

-- A row's display credits at their positions among its credits. The printed
-- name heads a picker's row; the artist, where the catalog names one, is who
-- is credited.
CREATE TABLE IF NOT EXISTS source_release_entry_credit (
    catalog               TEXT NOT NULL,
    release_id            TEXT NOT NULL,
    entry                 INTEGER NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    credited_name         TEXT NOT NULL,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (catalog, release_id, entry, position),
    FOREIGN KEY (catalog, release_id, entry)
        REFERENCES source_release_entry (catalog, release_id, entry) ON DELETE CASCADE,
    CHECK (name IS NOT NULL OR (sort_name IS NULL AND musicbrainz_artist_id IS NULL
        AND discogs_artist_id IS NULL))
) STRICT;

-- A row's composer credits, at their positions among its relations or
-- credits, with the words the catalog credits each with.
CREATE TABLE IF NOT EXISTS source_release_entry_role (
    catalog               TEXT NOT NULL,
    release_id            TEXT NOT NULL,
    entry                 INTEGER NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    role                  TEXT,
    PRIMARY KEY (catalog, release_id, entry, position),
    FOREIGN KEY (catalog, release_id, entry)
        REFERENCES source_release_entry (catalog, release_id, entry) ON DELETE CASCADE
) STRICT;

-- The MusicBrainz works a track performs, each as that reference states it:
-- a performed work hangs off its track at its position among the recording's
-- relations; a part hangs off its work at its position among that work's
-- relations, in the direction the relation runs.
CREATE TABLE IF NOT EXISTS source_release_work (
    catalog             TEXT NOT NULL CHECK (catalog = 'musicbrainz'),
    release_id          TEXT NOT NULL,
    node                INTEGER NOT NULL CHECK (node >= 0),
    entry               INTEGER,
    parent              INTEGER CHECK (parent IS NULL OR parent < node),
    position            INTEGER NOT NULL CHECK (position >= 0),
    direction           TEXT CHECK (direction IS NULL OR direction IN ('forward', 'backward')),
    musicbrainz_work_id TEXT NOT NULL,
    title               TEXT NOT NULL,
    disambiguation      TEXT,
    work_type           TEXT,
    PRIMARY KEY (catalog, release_id, node),
    FOREIGN KEY (catalog, release_id, entry)
        REFERENCES source_release_entry (catalog, release_id, entry) ON DELETE CASCADE,
    FOREIGN KEY (catalog, release_id, parent)
        REFERENCES source_release_work (catalog, release_id, node) ON DELETE CASCADE,
    CHECK ((entry IS NULL) <> (parent IS NULL)),
    CHECK ((parent IS NULL) = (direction IS NULL))
) STRICT;

-- A work's composers, at their positions among the work's relations.
CREATE TABLE IF NOT EXISTS source_release_work_composer (
    catalog               TEXT NOT NULL,
    release_id            TEXT NOT NULL,
    node                  INTEGER NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (catalog, release_id, node, position),
    FOREIGN KEY (catalog, release_id, node)
        REFERENCES source_release_work (catalog, release_id, node) ON DELETE CASCADE
) STRICT;

-- The supporting documents a fetch followed a link to and did not get, whose
-- facts the release's rows lack: 'failed' where the source was asked and
-- failed, 'discogs_not_configured' for a Discogs document with no key to ask
-- with. Fetching the release again replaces them with what it gets.
CREATE TABLE IF NOT EXISTS source_release_unfetched (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    document   TEXT NOT NULL,
    key        TEXT NOT NULL CHECK (key <> ''),
    reason     TEXT NOT NULL CHECK (reason IN ('failed', 'discogs_not_configured')),
    PRIMARY KEY (catalog, release_id, document, key),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;
