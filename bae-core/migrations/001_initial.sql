-- bae's schema, which coven applies as host migration 1 when it opens the
-- database. coven creates its own bookkeeping tables separately.

-- ── The library ───────────────────────────────────────────────────────────────

-- Every artist the library knows. A lookup matches an incoming artist by
-- provider id first, then by `name_key`: `name` with case, accents and spacing
-- folded by `text_match::artist_name_key`, always written together with `name`.
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

-- Two library artists the user confirmed are one: `id` was absorbed into
-- `into_artist_id`. Both rows stay so credits another device gave the absorbed
-- artist still resolve; reads go through `merged_artist_survivors`.
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

-- Each merged artist and the artist it shows as: the end of its merge chain.
-- A cycle, left by devices merging a pair in opposite directions, shows as its
-- smallest id. An artist absent here shows as itself.
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

-- One stored picture per artist; `id` is the artist's id.
CREATE TABLE IF NOT EXISTS artist_images (
    id TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    source_url TEXT,
    -- Cloud object key under the `artist_images` namespace: NULL where the home
    -- names blobs by id, `{artist}/artist.{ext}` on a browsable home.
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- Content hash, as on release_files.hash.
    hash TEXT NOT NULL,
    -- The coven blob holding the bytes; a new one per stored image, as on
    -- covers.blob_id.
    blob_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

-- An album groups releases; each pressing's own facts are on its `releases`
-- row.
CREATE TABLE IF NOT EXISTS albums (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    -- The first credited artist; the rest are in album_artists. Nullable here,
    -- but the app always sets it.
    artist_id TEXT REFERENCES artists(id),
    year INTEGER,
    -- The release whose cover the album shows and that opens by default; NULL
    -- means the first release.
    primary_release_id TEXT,
    is_compilation INTEGER NOT NULL DEFAULT 0,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_albums_artist_id ON albums (artist_id);

-- The album's artists after the first (`albums.artist_id`), in credit order.
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

-- One pressing of an album whose audio the library holds.
CREATE TABLE IF NOT EXISTS releases (
    id TEXT PRIMARY KEY,
    album_id TEXT NOT NULL,
    release_name TEXT,
    year INTEGER,
    -- Every label the pressing is on, in its source's order: a JSON array of
    -- {"name", "catalog_number"} objects, each stating one or both.
    labels             TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(labels) AND json_type(labels) = 'array'),
    barcode TEXT,
    -- Where the pressing was released: an ISO 3166-1 alpha-2 country code, or a
    -- `crate::pressing::Region` key for a region no current code names.
    country            TEXT CHECK (country IS NULL OR (length(country) = 2 AND country = upper(country))),
    region             TEXT CHECK (region IS NULL OR region <> ''),
    -- A JSON array of {"medium", "count"}, one per carrier in the record's
    -- order; empty where nothing is stated.
    media              TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(media) AND json_type(media) = 'array'),
    status             TEXT CHECK (status IS NULL OR status IN ('official', 'promotion', 'bootleg', 'pseudo_release', 'withdrawn', 'expunged', 'cancelled')),
    packaging          TEXT CHECK (packaging IS NULL OR packaging IN ('jewel_case', 'slim_jewel_case', 'digipak', 'cardboard_sleeve', 'other', 'keep_case', 'unpackaged', 'cassette_case', 'book', 'fatbox', 'snap_case', 'gatefold_cover', 'discbox_slider', 'super_jewel_box', 'digibook', 'plastic_sleeve', 'box', 'slidepack', 'snap_pack', 'metal_tin', 'longbox', 'clamshell_case', 'digifile', 'slipcase')),
    -- What Discogs says about the pressing that no column holds: a JSON array
    -- of `crate::pressing::DiscogsDetail` keys, each once, in its order.
    discogs_details    TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(discogs_details) AND json_type(discogs_details) = 'array'),
    -- Whether the audio is in the cloud home (1) or on this device only (0).
    -- coven syncs the release and the rows under it only when 1; a local
    -- release's files are tracked in coven's device-local `local_blob_refs`.
    remote INTEGER NOT NULL,
    source_folder_name TEXT,
    -- SHA-256 over the imported folder's sorted relative paths and sizes, so the
    -- same rip hashes the same wherever it sits. Recognizes an already-imported
    -- folder and picks the release a re-import overwrites.
    content_hash TEXT,
    -- EBU R128 integrated loudness over all tracks, in LUFS, measured at import;
    -- NULL when measuring failed. Playback derives the gain from it.
    album_loudness_lufs REAL,
    -- True peak as a linear ratio (1.0 = 0 dBTP), the max over all tracks;
    -- playback caps the gain at 1.0/peak.
    album_peak_linear REAL,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- Whether the stored metadata came from the folder's file tags rather than
    -- a catalog record.
    draft_from_tags INTEGER NOT NULL DEFAULT 0 CHECK (draft_from_tags IN (0, 1)),
    -- The catalog of the `release_records` entry the stored metadata came from.
    -- One value per release, so devices that chose different records settle on
    -- one.
    draft_catalog TEXT,
    CHECK (country IS NULL OR region IS NULL),
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_releases_album_id ON releases (album_id);

CREATE INDEX IF NOT EXISTS idx_releases_content_hash
    ON releases (content_hash)
    WHERE content_hash IS NOT NULL;

-- One stored cover per release; `id` is the release's id.
CREATE TABLE IF NOT EXISTS covers (
    id TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    source_url TEXT,
    -- Cloud object key under the `covers` namespace: NULL where the home names
    -- blobs by id, `{album}/{release}/cover.{ext}` on a browsable home.
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- Content hash, as on release_files.hash.
    hash TEXT NOT NULL,
    -- The coven blob holding the bytes. coven never rewrites a blob, so
    -- replacing a cover points the row at a new blob id.
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
    -- Cloud object key for the file's blob: NULL where the home names blobs by
    -- id, `{artist}/{album}/{filename}` on a browsable home. Set once at upload,
    -- so renaming metadata never moves the blob.
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- Lowercase-hex SHA-256 of the blob's plaintext; coven checks fetched bytes
    -- against it.
    hash TEXT NOT NULL,
    -- What the file held before import rewrote it: its own audio ('file') or
    -- one slice of a CUE-described disc ('cue').
    source_audio_layout TEXT,
    source_audio_content_type TEXT,
    source_audio_duration_ms INTEGER CHECK (source_audio_duration_ms IS NULL OR source_audio_duration_ms >= 0),
    source_audio_sample_rate_hz INTEGER CHECK (source_audio_sample_rate_hz IS NULL OR source_audio_sample_rate_hz > 0),
    source_audio_bits_per_sample INTEGER CHECK (source_audio_bits_per_sample IS NULL OR source_audio_bits_per_sample > 0),
    source_audio_bitrate_kbps INTEGER CHECK (source_audio_bitrate_kbps IS NULL OR source_audio_bitrate_kbps > 0),
    source_audio_channels INTEGER,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE,
    -- A file that is not audio states no audio facts. An audio file states
    -- them all, the codec deciding whether bits per sample or bitrate is set,
    -- and a layout unless the release carries it with the tracklist leaving
    -- it out.
    CHECK (
        (source_audio_layout IS NULL
            AND source_audio_content_type IS NULL
            AND source_audio_duration_ms IS NULL
            AND source_audio_sample_rate_hz IS NULL
            AND source_audio_bits_per_sample IS NULL
            AND source_audio_bitrate_kbps IS NULL
            AND source_audio_channels IS NULL)
        OR (
            (source_audio_layout IS NULL OR source_audio_layout IN ('file', 'cue'))
            AND source_audio_channels IS NOT NULL
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
    )
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

-- The catalog entries describing a release, one per catalog, each naming the
-- pressing or its album. `releases.draft_catalog` names the one in use.
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
    -- Playing order within the release, from zero across all sides.
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
    -- EBU R128 integrated loudness of the track, in LUFS, measured at import;
    -- NULL when measuring failed or the track is near-silent. Playback derives
    -- the gain from it.
    track_loudness_lufs REAL,
    -- True peak as a linear ratio (1.0 = 0 dBTP), the max over channels;
    -- playback caps the gain at 1.0/peak.
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

-- The one row saying what this device was playing, so a restart resumes it.
-- Never synced.
CREATE TABLE IF NOT EXISTS playback_state (
    id               TEXT PRIMARY KEY,
    source           TEXT,
    -- Whether playback from `source` was shuffled; NULL exactly when `source`
    -- is. The order is not stored: a restore shuffles again.
    shuffled         INTEGER,
    manual           TEXT NOT NULL,
    repeat           TEXT NOT NULL,
    current_track_id TEXT,
    position_ms      INTEGER,
    volume           REAL NOT NULL,
    is_muted         INTEGER NOT NULL
);

-- ── Watched folders and their scans ───────────────────────────────────────────

-- The folders the desktop app watches for releases to import, in the user's
-- order. Scan results belong to one watched folder (`folder_scan_roots` and
-- below). What is known about a folder under one — the user's decisions, and
-- when it was found — is keyed by its path on disk, so it outlives a takeover
-- and goes when no watched folder covers the folder.
CREATE TABLE IF NOT EXISTS watched_import_folders (
    path      TEXT PRIMARY KEY,
    position  INTEGER NOT NULL UNIQUE CHECK (position >= 0)
) STRICT;

-- Folders read as one release, or a folder whose releases are kept apart.
-- A grouping anchored at a folder reads the releases below it as one or keeps
-- them apart; the scan proposes one where a folder yields several releases,
-- and the user's answer replaces it. A grouping with no anchor combines the
-- releases its members name, from anywhere, and keeps them out of the queue
-- while it stands; its release is listed under its first member's watched
-- folder. Folders are named by their paths on disk, so a folder taking over
-- the watched folders inside it keeps their groupings; removing the watched
-- folder covering one deletes it (`remove_watched_import_folders`).
CREATE TABLE IF NOT EXISTS release_grouping (
    -- The candidate key of the release the grouping reads as one.
    key                  TEXT PRIMARY KEY,
    -- The folder an anchored grouping reads; NULL for one with no anchor.
    anchor_folder        TEXT UNIQUE,
    combined             INTEGER NOT NULL CHECK (combined IN (0, 1)),
    -- 'heuristic' when the scan decided because nothing was stored; 'user' for
    -- the user's answer, which the scan never overrides.
    author               TEXT NOT NULL CHECK (author IN ('user', 'heuristic')),
    skipped              INTEGER NOT NULL DEFAULT 0 CHECK (skipped IN (0, 1)),
    -- Why a grouping with no anchor cannot be worked on: a release it takes in
    -- changed or is gone, its folder's files go with another release or are
    -- still downloading, or its releases make no release. The release keeps
    -- what it was last built from until this is fixed or the grouping is
    -- undone. `blocked_subject` names the folder (for 'unbuildable', the
    -- diagnostic) and `blocked_holder` the release already reading the
    -- folder's files; both are for the log.
    blocked              TEXT CHECK (blocked IS NULL OR blocked IN (
        'source_changed', 'source_gone', 'folder_files_taken',
        'folder_files_contested', 'folder_files_downloading', 'unbuildable'
    )),
    blocked_subject      TEXT,
    blocked_holder       TEXT,
    -- For a grouping with no anchor: the folder all its releases sit directly
    -- in, and whether its release reads that folder's sidecar files
    -- (scan_sidecar). At most one release reads them: while several groupings
    -- sit in a folder with files a new one is refused, and a rebuild blocks
    -- all but the reader.
    parent_folder        TEXT,
    reads_parent_files   INTEGER NOT NULL DEFAULT 0 CHECK (reads_parent_files IN (0, 1)),
    CHECK (anchor_folder IS NOT NULL OR (combined = 1 AND author = 'user')),
    CHECK (anchor_folder IS NULL OR blocked IS NULL),
    CHECK ((blocked IS NULL) = (blocked_subject IS NULL)),
    CHECK ((blocked IS 'folder_files_taken') = (blocked_holder IS NOT NULL)),
    CHECK (anchor_folder IS NULL OR parent_folder IS NULL),
    CHECK (reads_parent_files = 0 OR parent_folder IS NOT NULL)
) STRICT;

-- The groupings sitting in a folder, and the one that reads its files.
CREATE INDEX IF NOT EXISTS release_grouping_by_parent ON release_grouping (parent_folder);

CREATE UNIQUE INDEX IF NOT EXISTS release_grouping_parent_reader
    ON release_grouping (parent_folder) WHERE reads_parent_files = 1;

-- The releases a grouping with no anchor takes in, in play order. A grouping
-- goes once no watched folder covers a folder it takes a release from.
CREATE TABLE IF NOT EXISTS release_grouping_member (
    grouping_key  TEXT NOT NULL
        REFERENCES release_grouping (key) ON DELETE CASCADE,
    position      INTEGER NOT NULL CHECK (position >= 0),
    member_key    TEXT NOT NULL UNIQUE,
    -- The folder on disk the member release is read from.
    member_folder TEXT NOT NULL,
    PRIMARY KEY (grouping_key, position)
) STRICT;

-- The candidates the user dismissed, by folder path, so a later scan does not
-- offer them again.
CREATE TABLE IF NOT EXISTS skipped_import_candidates (
    candidate_path TEXT PRIMARY KEY
) STRICT;

-- When the scans first saw each folder, the date the folder carries, and
-- whether a scan has read it as a release or as broken: a valid release read
-- where it has not is newly found.
CREATE TABLE IF NOT EXISTS folder_discovery (
    folder           TEXT PRIMARY KEY,
    first_seen_at    INTEGER NOT NULL,
    settled          INTEGER NOT NULL CHECK (settled IN (0, 1)),
    source_date      INTEGER,
    source_date_kind TEXT CHECK ((source_date IS NULL AND source_date_kind IS NULL)
        OR (source_date IS NOT NULL AND source_date_kind IS NOT NULL
            AND source_date_kind IN ('added_to_directory', 'created')))
) STRICT;

-- The one row handing out scan generations, so each root's generation is
-- stored before its scan begins.
CREATE TABLE IF NOT EXISTS folder_scan_generation_sequence (
    singleton       INTEGER PRIMARY KEY CHECK (singleton = 1),
    last_generation INTEGER NOT NULL CHECK (last_generation >= 0)
) STRICT;

INSERT OR IGNORE INTO folder_scan_generation_sequence (singleton, last_generation)
VALUES (1, 0);

-- This device's last scan of each watched folder. A scan writes entries as it
-- finds them; completing removes the ones it did not see, in the transaction
-- that marks it complete. A failed or interrupted scan keeps old and new ones.
CREATE TABLE IF NOT EXISTS folder_scan_roots (
    watched_folder_path TEXT PRIMARY KEY,
    generation          INTEGER NOT NULL CHECK (generation >= 0),
    status              TEXT NOT NULL CHECK (status IN ('scanning', 'complete', 'failed')),
    error               TEXT,
    -- The volume the folder was on when the scan began, asked once per scan so
    -- reading scan state never waits on a mount.
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
    -- 'grouping' for a release a grouping with no anchor builds from its
    -- members.
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
        (role <> 'audio' AND audio_content_type IS NULL AND audio_duration_ms IS NULL
            AND audio_sample_rate_hz IS NULL AND audio_bits_per_sample IS NULL
            AND audio_bitrate_kbps IS NULL AND audio_channels IS NULL)
        OR
        (role = 'audio' AND audio_content_type IS NOT NULL AND audio_duration_ms IS NOT NULL
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
    -- The ISRC the file's recording is registered under, as its tag writes it.
    isrc                TEXT CHECK (isrc IS NULL OR isrc <> ''),
    label               TEXT CHECK (label IS NULL OR label <> ''),
    copyright           TEXT CHECK (copyright IS NULL OR copyright <> ''),
    -- The store the tags say sold the file: 'itunes_purchase', the atoms iTunes
    -- writes only into a purchase; 'bandcamp', Bandcamp's "Visit" comment.
    store               TEXT CHECK (store IS NULL OR store IN ('itunes_purchase', 'bandcamp')),
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

-- The files under a folder that no release the scan read there owns, such as
-- a cover or booklet beside disc folders kept as separate releases. Stored and
-- pruned like the scan's candidates. A sidecar and a scanned release holding
-- the same file replace each other, whichever is written later. Watched roots
-- never overlap, so the folder alone is the key.
CREATE TABLE IF NOT EXISTS scan_sidecar (
    folder              TEXT PRIMARY KEY,
    watched_folder_path TEXT NOT NULL,
    generation          INTEGER NOT NULL CHECK (generation >= 0),
    -- 'invalid' when one of its files is broken, so a release taking them in
    -- cannot be imported; 'downloading' while a download into the folder runs.
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
    -- The CD ripper that wrote the sheet, where its REM COMMENT names one.
    ripper              TEXT CHECK (ripper IS NULL OR ripper IN ('exact_audio_copy')),
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE
) STRICT;

-- One track of a CUE sheet, with the span it declares and the silence a PREGAP
-- directive puts before it. An audio pregap is the track's INDEX 00.
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
    generated_pregap_frames     INTEGER CHECK (generated_pregap_frames IS NULL OR generated_pregap_frames >= 0),
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path, position),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_cue_sheet (watched_folder_path, candidate_path, sheet_relative_path) ON DELETE CASCADE,
    CHECK ((mode = 'other') = (mode_other IS NOT NULL))
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

-- The candidates the person has selected in the import list, by candidate key.
-- Its lifetime is one app session: emptied when the library opens. A key
-- leaves in the same write that removes its release from the queue, imports
-- its files, or moves it between Pending and Skipped.
CREATE TABLE IF NOT EXISTS candidate_selection (
    candidate_key TEXT PRIMARY KEY
) STRICT, WITHOUT ROWID;

-- The one row counting the person's changes to the selection, so a list read
-- says which of them it reflects.
CREATE TABLE IF NOT EXISTS candidate_selection_revision (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    revision  INTEGER NOT NULL CHECK (revision >= 0)
) STRICT;

INSERT OR IGNORE INTO candidate_selection_revision (singleton, revision)
VALUES (1, 0);

CREATE TRIGGER IF NOT EXISTS deselect_removed_candidate AFTER DELETE ON scan_candidate
WHEN NOT EXISTS (SELECT 1 FROM scan_candidate WHERE path = OLD.path)
BEGIN
    DELETE FROM candidate_selection WHERE candidate_key = OLD.path;
END;

CREATE TRIGGER IF NOT EXISTS deselect_imported_candidate AFTER INSERT ON releases
WHEN NEW.content_hash IS NOT NULL
BEGIN
    DELETE FROM candidate_selection WHERE candidate_key IN
        (SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS deselect_reimported_candidate AFTER UPDATE OF content_hash ON releases
WHEN NEW.content_hash IS NOT NULL
BEGIN
    DELETE FROM candidate_selection WHERE candidate_key IN
        (SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS deselect_skipped_candidate AFTER INSERT ON skipped_import_candidates
BEGIN
    DELETE FROM candidate_selection WHERE candidate_key = NEW.candidate_path;
END;

CREATE TRIGGER IF NOT EXISTS deselect_restored_candidate AFTER DELETE ON skipped_import_candidates
BEGIN
    DELETE FROM candidate_selection WHERE candidate_key = OLD.candidate_path;
END;

CREATE TRIGGER IF NOT EXISTS deselect_skipped_grouping AFTER UPDATE OF skipped ON release_grouping
WHEN NEW.skipped != OLD.skipped
BEGIN
    DELETE FROM candidate_selection WHERE candidate_key = NEW.key;
END;

-- ── Import candidates ─────────────────────────────────────────────────────────

-- One folder being imported, keyed by the hash of its file layout. The other
-- import_candidate tables hang off it.
CREATE TABLE IF NOT EXISTS import_candidate_state (
    content_hash      TEXT PRIMARY KEY,
    -- Where the candidate was last seen; not its identity.
    folder_path       TEXT NOT NULL,
    -- Advances with every draft or cover change; commands return it so a
    -- surface can wait for exactly that committed state.
    metadata_revision INTEGER NOT NULL DEFAULT 0 CHECK (metadata_revision >= 0),
    -- Advances with every file decision, so a verdict based on older files is
    -- refused.
    edit_revision     INTEGER NOT NULL DEFAULT 0 CHECK (edit_revision >= 0)
) STRICT;

-- The folders on disk a candidate was found at (several when the same files
-- sit in two places). The candidate lives while a watched folder covers one.
CREATE TABLE IF NOT EXISTS import_candidate_folder (
    content_hash TEXT NOT NULL,
    folder       TEXT NOT NULL,
    PRIMARY KEY (content_hash, folder),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The release-level metadata draft the import will write.
CREATE TABLE IF NOT EXISTS import_candidate_edit (
    content_hash   TEXT PRIMARY KEY,
    album_title    TEXT NOT NULL,
    album_year     TEXT NOT NULL,
    year           TEXT NOT NULL,
    -- The label rows as the form holds them: a JSON array of
    -- {"name", "catalog_number"} objects, each as typed, empty meaning unset.
    labels         TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(labels) AND json_type(labels) = 'array'),
    barcode        TEXT NOT NULL,
    -- Where the pressing was released: an ISO 3166-1 alpha-2 country code, or a
    -- `crate::pressing::Region` key for a region no current code names.
    country            TEXT CHECK (country IS NULL OR (length(country) = 2 AND country = upper(country))),
    region             TEXT CHECK (region IS NULL OR region <> ''),
    -- A JSON array of {"medium", "count"}, one per carrier in the record's
    -- order; empty where nothing is stated.
    media              TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(media) AND json_type(media) = 'array'),
    status             TEXT CHECK (status IS NULL OR status IN ('official', 'promotion', 'bootleg', 'pseudo_release', 'withdrawn', 'expunged', 'cancelled')),
    packaging          TEXT CHECK (packaging IS NULL OR packaging IN ('jewel_case', 'slim_jewel_case', 'digipak', 'cardboard_sleeve', 'other', 'keep_case', 'unpackaged', 'cassette_case', 'book', 'fatbox', 'snap_case', 'gatefold_cover', 'discbox_slider', 'super_jewel_box', 'digibook', 'plastic_sleeve', 'box', 'slidepack', 'snap_pack', 'metal_tin', 'longbox', 'clamshell_case', 'digifile', 'slipcase')),
    -- What Discogs says about the pressing that no column holds: a JSON array
    -- of `crate::pressing::DiscogsDetail` keys, each once, in its order.
    discogs_details    TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(discogs_details) AND json_type(discogs_details) = 'array'),
    -- Who wrote the draft: nobody (the blank one discovery creates), discovery
    -- from the folder's tags ('prefill'), an identification run, or a person.
    -- Which provenance goes with each is checked in code, since it spans this
    -- row and the provenance row.
    author         TEXT NOT NULL
        CHECK (author IN ('nobody', 'prefill', 'identification', 'person')),
    -- Whether the draft is blank, and whether it is complete and valid; written
    -- with the draft so the import list need not read every draft whole.
    draft_blank    INTEGER NOT NULL CHECK (draft_blank IN (0, 1)),
    draft_valid    INTEGER NOT NULL CHECK (draft_valid IN (0, 1)),
    CHECK (country IS NULL OR region IS NULL),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The draft's album artists: a library artist the user picked ('picked'), or
-- a name a source or the user gave ('credit'). Which library artist a credit
-- is, if any, is decided each time the draft is read and again on import.
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

-- The draft's per-track artists, where a track does not take the album's;
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

-- Which disc of the release the user said each CUE sheet is, or that it is
-- ignored.
CREATE TABLE IF NOT EXISTS import_candidate_sheet_disc (
    content_hash TEXT NOT NULL,
    sheet_id     TEXT NOT NULL,
    disc         TEXT NOT NULL CHECK (disc IN ('disc', 'ignored')),
    disc_number  INTEGER CHECK (disc_number IS NULL OR disc_number >= 1),
    PRIMARY KEY (content_hash, sheet_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state(content_hash) ON DELETE CASCADE,
    CHECK ((disc = 'disc') = (disc_number IS NOT NULL))
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
    -- Referenced by import_candidate_cover_copy; only a remote cover has a url.
    UNIQUE (content_hash, url)
) STRICT;

-- The downscaled copies the catalog serves of a remote cover, one per size (a
-- copy's longer side is at most max_edge pixels). A local or embedded cover
-- has no url, so it has none.
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

-- Where the draft was read from; who wrote it is `import_candidate_edit.author`.
CREATE TABLE IF NOT EXISTS import_candidate_draft_provenance (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('external_release', 'file_tags')),
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    release_id   TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (source, release_id) REFERENCES source_release (catalog, release_id),
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

-- Marks that the draft's tracks point into the tracklists of the releases its
-- provenance names, as laid out against import_candidate_applied_length.
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
    find_online_section TEXT NOT NULL CHECK (find_online_section IN ('automatic', 'search')),
    search_tab     TEXT NOT NULL CHECK (search_tab IN ('general', 'catalog_number', 'barcode')),
    search_artist  TEXT NOT NULL,
    search_album   TEXT NOT NULL,
    search_catalog TEXT NOT NULL,
    search_barcode TEXT NOT NULL,
    -- The pane's last command, when it failed: which command, the class of
    -- its failure, and the failure's untranslated text.
    error_command  TEXT CHECK (error_command IN (
        'import', 'merge_artists', 'read_file_tags',
        'change_lookups', 'change_search_words', 'change_agreements', 'keep_own_draft')),
    error_category TEXT,
    error_detail   TEXT,
    CHECK ((error_command IS NULL) = (error_category IS NULL)),
    CHECK ((error_command IS NULL) = (error_detail IS NULL)),
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

-- What extraction read off a candidate: what its files say about the medium
-- it was ripped from, its disc ID, and whether the barcode and text passes
-- settled or failed.
CREATE TABLE IF NOT EXISTS import_candidate_signals (
    content_hash           TEXT PRIMARY KEY,
    -- Where a file proves the audio came from: a CD rip or a download, or NULL
    -- when nothing proves either.
    audio_source           TEXT CHECK (audio_source IS NULL OR audio_source IN ('cd_rip', 'download')),
    cd_rip_proof           TEXT CHECK (cd_rip_proof IS NULL OR cd_rip_proof IN ('rip_log', 'accurate_rip_report', 'ripper_sheet')),
    download_proof         TEXT CHECK (download_proof IS NULL OR download_proof IN ('itunes_purchase', 'bandcamp', 'delivery_set')),
    -- The candidate-relative path of the file that proves the source: the rip
    -- file, or the track a store marked. NULL for a label's delivery set, and
    -- when re-identifying a library release.
    audio_source_file      TEXT,
    -- The lossless rate, off a CD's, that rules a CD out.
    not_cd_rate            INTEGER CHECK (not_cd_rate IS NULL OR not_cd_rate > 0),
    disc_id_state          TEXT NOT NULL CHECK (disc_id_state IN ('computed', 'absent', 'not_cd_audio', 'failed')),
    disc_id                TEXT,
    -- The candidate-relative path of the LOG or CUE the disc ID came from, so a
    -- surface can mark that file's row. NULL when re-identifying a library
    -- release, whose ID comes from stored tracks.
    disc_id_source_file    TEXT,
    -- How bae broke reading each signal off the folder's files, as the error
    -- chain the person is shown.
    disc_id_failure        TEXT CHECK (disc_id_failure IS NULL OR disc_id_failure <> ''),
    barcode_state          TEXT NOT NULL CHECK (barcode_state IN ('settled', 'failed', 'absent')),
    barcode_failure        TEXT CHECK (barcode_failure IS NULL OR barcode_failure <> ''),
    text_state             TEXT NOT NULL CHECK (text_state IN ('settled', 'failed')),
    text_failure           TEXT CHECK (text_failure IS NULL OR text_failure <> ''),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((audio_source IS 'cd_rip') = (cd_rip_proof IS NOT NULL)),
    CHECK ((audio_source IS 'download') = (download_proof IS NOT NULL)),
    CHECK (download_proof IS NOT 'delivery_set' OR audio_source_file IS NULL),
    CHECK (download_proof NOT IN ('itunes_purchase', 'bandcamp') OR audio_source_file IS NOT NULL),
    CHECK (audio_source IS NOT NULL OR audio_source_file IS NULL),
    -- A sheet goes unhashed only when the audio rules a CD out and nothing
    -- proves a CD rip.
    CHECK (disc_id_state <> 'not_cd_audio'
        OR (not_cd_rate IS NOT NULL AND audio_source IS NOT 'cd_rip')),
    CHECK ((disc_id_state = 'computed') = (disc_id IS NOT NULL)),
    CHECK (disc_id_source_file IS NULL OR disc_id_state = 'computed'),
    CHECK ((disc_id_state = 'failed') = (disc_id_failure IS NOT NULL)),
    CHECK ((barcode_state = 'failed') = (barcode_failure IS NOT NULL)),
    CHECK ((text_state = 'failed') = (text_failure IS NOT NULL))
) STRICT;

-- The barcodes, catalog numbers and free-text candidates read off a candidate,
-- in reading order.
CREATE TABLE IF NOT EXISTS import_candidate_signal_value (
    content_hash TEXT NOT NULL,
    list         TEXT NOT NULL CHECK (list IN ('barcode', 'catalog', 'free_text', 'isrc', 'track_title')),
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT NOT NULL,
    -- The candidate-relative path of the file a barcode was read off; NULL for
    -- a library release's stored images.
    origin_path  TEXT CHECK (origin_path IS NULL OR list = 'barcode'),
    PRIMARY KEY (content_hash, list, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_signals (content_hash) ON DELETE CASCADE
) STRICT;

-- Every line of text read off a candidate, from which ranking reads the
-- folder's own claims.
CREATE TABLE IF NOT EXISTS import_candidate_text_line (
    content_hash  TEXT NOT NULL,
    position      INTEGER NOT NULL CHECK (position >= 0),
    text          TEXT NOT NULL,
    origin        TEXT NOT NULL
        CHECK (origin IN ('cue_sheet', 'artwork', 'folder_name', 'filename', 'text_file',
                          'file_tag')),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_signals (content_hash) ON DELETE CASCADE
) STRICT;

-- Which values read off a candidate the user let the lookups use, and the
-- title search words the user typed instead of the draft's (both NULL to search
-- the draft's title).
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

-- What an identify run concluded.
CREATE TABLE IF NOT EXISTS import_candidate_verdict (
    content_hash  TEXT PRIMARY KEY,
    kind          TEXT NOT NULL
        CHECK (kind IN ('found', 'not_found', 'manual_only', 'failed', 'error')),
    -- How many tracks the folder played when the verdict was reached.
    track_count   INTEGER CHECK (track_count IS NULL OR track_count >= 0),
    -- The lookup failures of a failed verdict, stored as JSON since no query
    -- reads into them.
    failures_json TEXT CHECK (
        failures_json IS NULL
        OR (json_valid(failures_json)
            AND json_type(failures_json) = 'array'
            AND json_array_length(failures_json) > 0)
    ),
    -- The ledger the run recorded as it ended, stored whole since no query
    -- reads into it; NULL when none was recorded.
    ledger_json   TEXT CHECK (
        ledger_json IS NULL
        OR (json_valid(ledger_json) AND json_type(ledger_json) = 'object')
    ),
    -- How bae broke on its own side, for an 'error' verdict: the error chain
    -- the person is shown.
    error_detail  TEXT CHECK (error_detail IS NULL OR error_detail <> ''),
    -- The person kept their own draft over what this verdict offered: none of
    -- its releases is the folder's, or it found none. It goes with the
    -- verdict, so the next identification starts without it.
    kept_own_draft INTEGER NOT NULL CHECK (kept_own_draft IN (0, 1)),
    identified_at TEXT NOT NULL,
    -- The folder's own files rule out every row found: a CD rip where no row
    -- could be a CD, or a sample rate no CD holds where every row is a CD.
    -- Such a verdict is never Ready.
    medium_conflict TEXT CHECK (medium_conflict IS NULL OR medium_conflict IN ('cd_rip', 'not_cd_audio')),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((kind IN ('not_found', 'error')) = (track_count IS NULL)),
    CHECK ((kind = 'failed') = (failures_json IS NOT NULL)),
    CHECK ((kind = 'error') = (error_detail IS NOT NULL)),
    CHECK (medium_conflict IS NULL OR kind IN ('found', 'failed'))
) STRICT;

-- Every release a run's lookups returned, in listed order, with what the
-- record said and which lookup found it.
CREATE TABLE IF NOT EXISTS import_candidate_match (
    content_hash        TEXT NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    -- The pressing row this release belongs to, numbered from zero; matches
    -- and narrowed-out releases each number their own rows.
    pressing            INTEGER NOT NULL CHECK (pressing >= 0),
    source              TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id          TEXT NOT NULL,
    title               TEXT NOT NULL,
    artist              TEXT,
    year                INTEGER,
    -- Every label the pressing is on, in its source's order: a JSON array of
    -- {"name", "catalog_number"} objects, each stating one or both.
    labels             TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(labels) AND json_type(labels) = 'array'),
    -- Where the pressing was released: an ISO 3166-1 alpha-2 country code, or a
    -- `crate::pressing::Region` key for a region no current code names.
    country            TEXT CHECK (country IS NULL OR (length(country) = 2 AND country = upper(country))),
    region             TEXT CHECK (region IS NULL OR region <> ''),
    status             TEXT CHECK (status IS NULL OR status IN ('official', 'promotion', 'bootleg', 'pseudo_release', 'withdrawn', 'expunged', 'cancelled')),
    packaging          TEXT CHECK (packaging IS NULL OR packaging IN ('jewel_case', 'slim_jewel_case', 'digipak', 'cardboard_sleeve', 'other', 'keep_case', 'unpackaged', 'cassette_case', 'book', 'fatbox', 'snap_case', 'gatefold_cover', 'discbox_slider', 'super_jewel_box', 'digibook', 'plastic_sleeve', 'box', 'slidepack', 'snap_pack', 'metal_tin', 'longbox', 'clamshell_case', 'digifile', 'slipcase')),
    -- What Discogs says about the pressing that no column holds: a JSON array
    -- of `crate::pressing::DiscogsDetail` keys, each once, in its order.
    discogs_details    TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(discogs_details) AND json_type(discogs_details) = 'array'),
    -- 'undescribed': the record described no media and there are no medium
    -- rows. 'per_medium': one row per medium listed, its medium NULL where bae
    -- does not know the carrier. 'formats': one row per format entry that is a
    -- medium, with its quantity.
    media_kind          TEXT NOT NULL
        CHECK (media_kind IN ('undescribed', 'per_medium', 'formats')),
    -- The lead cover's original; its downscaled copies are in
    -- import_candidate_match_cover_copy.
    cover_url           TEXT,
    cover_label         TEXT,
    cover_source        TEXT CHECK (cover_source IS NULL OR cover_source IN ('musicbrainz', 'discogs')),
    -- 'stated': the record's catalog says the image exists. 'unstated': an
    -- address the record said nothing about.
    cover_standing      TEXT CHECK (cover_standing IS NULL OR cover_standing IN ('stated', 'unstated')),
    source_group_id     TEXT,
    -- What the record's catalog says its album is on the other catalog.
    -- 'not_asked': never read. 'read': the album link rows hold what it named.
    -- 'unread': a needed document could not be fetched and nothing named an
    -- album.
    album_links         TEXT NOT NULL CHECK (album_links IN ('not_asked', 'read', 'unread')),
    -- NULL until the source is asked for its tracklist.
    source_tracks_kind  TEXT CHECK (source_tracks_kind IS NULL OR source_tracks_kind IN ('listed', 'nothing')),
    source_tracks_count INTEGER CHECK (source_tracks_count IS NULL OR source_tracks_count >= 0),
    -- Which lookup returned this release. Matches against the folder's text
    -- are not stored; they are read from the text lines each time, so
    -- re-ranking needs no new run.
    by_disc_id          INTEGER NOT NULL CHECK (by_disc_id IN (0, 1)),
    by_barcode          INTEGER NOT NULL CHECK (by_barcode IN (0, 1)),
    by_catalog          INTEGER NOT NULL CHECK (by_catalog IN (0, 1)),
    -- The search by the ISRCs the audio's tags carry.
    by_isrc             INTEGER NOT NULL CHECK (by_isrc IN (0, 1)),
    -- The title search, asked only when the disc ID, the barcodes and the
    -- catalog numbers all came back empty.
    by_search           INTEGER NOT NULL CHECK (by_search IN (0, 1)),
    -- For a release no lookup returned: the release whose own document names
    -- this one as the same release, read through to learn its album.
    named_by_catalog    TEXT CHECK (named_by_catalog IS NULL OR named_by_catalog <> ''),
    named_by_key        TEXT CHECK (named_by_key IS NULL OR named_by_key <> ''),
    narrowed_out        INTEGER NOT NULL DEFAULT 0 CHECK (narrowed_out IN (0, 1)),
    -- Why the release's full document could not be read when the run offered
    -- its row, which then holds what the lookup returned. NULL when it was
    -- read or never asked for.
    document_failure        TEXT CHECK (document_failure IS NULL OR document_failure IN ('network', 'provider', 'timeout')),
    document_failure_status INTEGER,
    -- The year the release's album first came out, as its full document's
    -- release group or master states it.
    album_first_year        INTEGER,
    -- The title of each track the full document lists for the audio, in
    -- order: a JSON array of strings, empty where it was not read or leaves a
    -- track untitled.
    track_titles            TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(track_titles) AND json_type(track_titles) = 'array'),
    -- What the full document writes about which pressing it is, in free text:
    -- a JSON array of strings, empty where it was not read or writes none.
    notes                   TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(notes) AND json_type(notes) = 'array'),
    -- The note of this release the folder's text names its row by, where the
    -- ranking's notes point went to that row; NULL otherwise, and always for a
    -- release set aside.
    named_note              TEXT CHECK (named_note IS NULL OR named_note <> ''),
    PRIMARY KEY (content_hash, position),
    -- Referenced by the medium rows with the media kind, so a medium row always
    -- belongs to a match of its kind.
    UNIQUE (content_hash, position, media_kind),
    -- Referenced by the cover copies; only a match with a cover has one.
    UNIQUE (content_hash, position, cover_url),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_verdict (content_hash) ON DELETE CASCADE,
    CHECK ((cover_url IS NULL) = (cover_label IS NULL) AND (cover_url IS NULL) = (cover_source IS NULL) AND (cover_url IS NULL) = (cover_standing IS NULL)),
    CHECK ((source_tracks_kind = 'listed') = (source_tracks_count IS NOT NULL)),
    CHECK ((named_by_catalog IS NULL) = (named_by_key IS NULL)),
    CHECK (narrowed_out = 0 OR named_note IS NULL),
    CHECK (named_by_catalog IS NULL
           OR (by_disc_id = 0 AND by_barcode = 0 AND by_catalog = 0 AND by_isrc = 0
               AND by_search = 0)),
    CHECK (country IS NULL OR region IS NULL),
    CHECK (document_failure_status IS NULL OR document_failure = 'provider')
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

-- The downscaled copies the catalog serves of a match's cover, one per size (a
-- copy's longer side is at most max_edge pixels).
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

-- Every other catalog's album a statement names as a matched record's album
-- (the Discogs masters a MusicBrainz release group is), for a match whose
-- album_links is 'read'. `stated` says how: 'page', the group's page links it;
-- 'wikidata', the Wikidata item the group's page links states it; 'release',
-- a release of the group (`musicbrainz_release`) links a twin release
-- (`release_catalog`, `release_key`) whose own document files it under the
-- album. Where none of those names an album, the list the group was read with
-- may: 'barcode', a release of the group on the list and a release of the
-- album on the list (`release_catalog`, `release_key`) print one barcode;
-- 'catalog_number', they print one catalog number under one label.
CREATE TABLE IF NOT EXISTS import_candidate_match_album_link (
    content_hash        TEXT NOT NULL,
    position            INTEGER NOT NULL,
    ordinal             INTEGER NOT NULL CHECK (ordinal >= 0),
    catalog             TEXT NOT NULL CHECK (catalog <> ''),
    key                 TEXT NOT NULL CHECK (key <> ''),
    stated              TEXT NOT NULL
        CHECK (stated IN ('page', 'wikidata', 'release', 'barcode', 'catalog_number')),
    wikidata_item       TEXT CHECK (wikidata_item IS NULL OR wikidata_item <> ''),
    musicbrainz_release TEXT CHECK (musicbrainz_release IS NULL OR musicbrainz_release <> ''),
    release_catalog     TEXT CHECK (release_catalog IS NULL OR release_catalog <> ''),
    release_key         TEXT CHECK (release_key IS NULL OR release_key <> ''),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match (content_hash, position) ON DELETE CASCADE,
    CHECK ((stated = 'wikidata') = (wikidata_item IS NOT NULL)),
    CHECK ((stated IN ('release', 'barcode', 'catalog_number')) = (musicbrainz_release IS NOT NULL)),
    CHECK ((stated IN ('release', 'barcode', 'catalog_number')) = (release_catalog IS NOT NULL)),
    CHECK ((stated IN ('release', 'barcode', 'catalog_number')) = (release_key IS NOT NULL))
) STRICT;

-- A matched record's media, one row per medium or format entry as the match's
-- media kind says: the carrier (a `crate::pressing::Medium` key, NULL where bae
-- does not know it) and how many.
CREATE TABLE IF NOT EXISTS import_candidate_match_medium (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    media_kind   TEXT NOT NULL CHECK (media_kind IN ('per_medium', 'formats')),
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    medium       TEXT CHECK (medium IS NULL OR medium <> ''),
    quantity     INTEGER NOT NULL CHECK (quantity >= 1),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position, media_kind)
        REFERENCES import_candidate_match (content_hash, position, media_kind)
        ON DELETE CASCADE,
    CHECK (media_kind = 'formats' OR quantity = 1)
) STRICT;

-- ── Catalog releases ──────────────────────────────────────────────────────────

-- One catalog release bae fetched, with every fact the import reads from it
-- extracted at fetch time. Device-local, since any device can fetch it; a new
-- fetch replaces every row under it at once.
CREATE TABLE IF NOT EXISTS source_release (
    catalog            TEXT NOT NULL CHECK (catalog IN ('musicbrainz', 'discogs')),
    release_id         TEXT NOT NULL CHECK (release_id <> ''),
    -- The album its catalog files it under: a MusicBrainz release group or a
    -- Discogs master.
    source_group_id    TEXT,
    -- The album's facts: the release's own, or where it states none, those of
    -- its cross-referenced release and its album's documents.
    album_title        TEXT NOT NULL,
    album_year         INTEGER,
    -- The year the album first came out, as its release group's first release
    -- date or its master's year states it; never a pressing's year.
    album_first_year   INTEGER,
    -- The pressing's facts, filled the same way.
    year               INTEGER,
    -- Every label the pressing is on, in its source's order: a JSON array of
    -- {"name", "catalog_number"} objects, each stating one or both.
    labels             TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(labels) AND json_type(labels) = 'array'),
    barcode            TEXT,
    -- Where the pressing was released: an ISO 3166-1 alpha-2 country code, or a
    -- `crate::pressing::Region` key for a region no current code names.
    country            TEXT CHECK (country IS NULL OR (length(country) = 2 AND country = upper(country))),
    region             TEXT CHECK (region IS NULL OR region <> ''),
    -- A JSON array of {"medium", "count"}, one per carrier in the record's
    -- order; empty where nothing is stated.
    media              TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(media) AND json_type(media) = 'array'),
    status             TEXT CHECK (status IS NULL OR status IN ('official', 'promotion', 'bootleg', 'pseudo_release', 'withdrawn', 'expunged', 'cancelled')),
    packaging          TEXT CHECK (packaging IS NULL OR packaging IN ('jewel_case', 'slim_jewel_case', 'digipak', 'cardboard_sleeve', 'other', 'keep_case', 'unpackaged', 'cassette_case', 'book', 'fatbox', 'snap_case', 'gatefold_cover', 'discbox_slider', 'super_jewel_box', 'digibook', 'plastic_sleeve', 'box', 'slidepack', 'snap_pack', 'metal_tin', 'longbox', 'clamshell_case', 'digifile', 'slipcase')),
    -- What Discogs says about the pressing that no column holds: a JSON array
    -- of `crate::pressing::DiscogsDetail` keys, each once, in its order.
    discogs_details    TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(discogs_details) AND json_type(discogs_details) = 'array'),
    -- What the release's own document writes about which pressing it is, in
    -- free text: a JSON array of strings — a MusicBrainz release's
    -- disambiguation; a Discogs release's company names, then its matrix /
    -- runout inscriptions.
    notes              TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(notes) AND json_type(notes) = 'array'),
    -- The MusicBrainz release whose Cover Art Archive gallery the picker opens
    -- (this release, or the one a Discogs release cross-references), and its
    -- release group.
    archive_release_id TEXT,
    archive_group_id   TEXT,
    fetched_at         TEXT NOT NULL,
    PRIMARY KEY (catalog, release_id),
    CHECK (archive_release_id IS NOT NULL OR archive_group_id IS NULL),
    CHECK (country IS NULL OR region IS NULL)
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

-- A Discogs release's format entries that are media, in order: the carrier (a
-- `crate::pressing::Medium` key, NULL where bae does not know it) and the
-- stated quantity.
CREATE TABLE IF NOT EXISTS source_release_format (
    catalog    TEXT NOT NULL CHECK (catalog = 'discogs'),
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    medium     TEXT CHECK (medium IS NULL OR medium <> ''),
    quantity   INTEGER NOT NULL CHECK (quantity >= 1),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- What another catalog says this release is ('pressing', with the album it
-- files it under) or what its album is ('album'). The release's own catalog is
-- the source_release row itself.
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

-- What reading a MusicBrainz release group found it to be on another catalog:
-- each album a statement names, in the columns import_candidate_match_album_link
-- uses. A stored release of either album reads the other as one of its
-- records, even when its own documents never reach it. Device-local; reading
-- the group again replaces its rows.
CREATE TABLE IF NOT EXISTS release_group_album_link (
    release_group       TEXT NOT NULL CHECK (release_group <> ''),
    catalog             TEXT NOT NULL CHECK (catalog <> '' AND catalog <> 'musicbrainz'),
    key                 TEXT NOT NULL CHECK (key <> ''),
    stated              TEXT NOT NULL
        CHECK (stated IN ('page', 'wikidata', 'release', 'barcode', 'catalog_number')),
    wikidata_item       TEXT CHECK (wikidata_item IS NULL OR wikidata_item <> ''),
    musicbrainz_release TEXT CHECK (musicbrainz_release IS NULL OR musicbrainz_release <> ''),
    release_catalog     TEXT CHECK (release_catalog IS NULL OR release_catalog <> ''),
    release_key         TEXT CHECK (release_key IS NULL OR release_key <> ''),
    PRIMARY KEY (release_group, catalog, key),
    CHECK ((stated = 'wikidata') = (wikidata_item IS NOT NULL)),
    CHECK ((stated IN ('release', 'barcode', 'catalog_number')) = (musicbrainz_release IS NOT NULL)),
    CHECK ((stated IN ('release', 'barcode', 'catalog_number')) = (release_catalog IS NOT NULL)),
    CHECK ((stated IN ('release', 'barcode', 'catalog_number')) = (release_key IS NOT NULL))
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
    -- The original; its downscaled copies are in source_release_cover_copy.
    url           TEXT NOT NULL,
    label         TEXT NOT NULL,
    source        TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    -- Whether a catalog says the image exists, or it is an address nothing
    -- said anything about.
    standing      TEXT NOT NULL CHECK (standing IN ('stated', 'unstated')),
    PRIMARY KEY (catalog, release_id, scope, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- The downscaled copies the catalog serves of one offered image, one per size
-- (a copy's longer side is at most max_edge pixels).
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

-- Every medium of the release, in order. Only MusicBrainz states a carrier (a
-- `crate::pressing::Medium` key, NULL where bae does not know it); a Discogs
-- release's media are the runs of rows its positions number as one disc.
CREATE TABLE IF NOT EXISTS source_release_medium (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    medium     TEXT CHECK (medium IS NULL OR medium <> ''),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- One row of a medium's tracklist. `entry` numbers the release's rows in
-- tracklist order, a Discogs index's sub-tracks right after it with the index
-- as parent. A 'heading' titles the rows after it.
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

-- The MusicBrainz works a track performs: a work hangs off its track at its
-- position among the recording's relations; a part hangs off its work at its
-- position among that work's relations, in the relation's direction.
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

-- The linked documents a fetch did not get, whose facts the release's rows
-- lack: 'failed' when the source was asked and failed,
-- 'discogs_not_configured' for a Discogs document with no key to ask with. A
-- new fetch replaces them.
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
