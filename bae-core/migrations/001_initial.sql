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

    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artists_name ON artists (name COLLATE NOCASE);

CREATE INDEX IF NOT EXISTS idx_artists_name_key ON artists (name_key);

CREATE INDEX IF NOT EXISTS idx_artists_discogs_id ON artists (discogs_artist_id);

CREATE INDEX IF NOT EXISTS idx_artists_mb_id ON artists (musicbrainz_artist_id);

-- An artist's other printed names: a name a credit prints for it that is not
-- its own (`artists.name`). A row is keyed by the artist and the folded name,
-- so two devices meeting one printed name write one row.
CREATE TABLE IF NOT EXISTS artist_credits (
    artist_id TEXT NOT NULL,
    name_key TEXT NOT NULL,
    name TEXT NOT NULL CHECK (name <> ''),
    created_at TEXT NOT NULL,
    PRIMARY KEY (artist_id, name_key),
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

-- Two library artists the user confirmed are one: `id` was absorbed into
-- `into_artist_id`. Both rows stay so credits another device gave the absorbed
-- artist still resolve; reads go through `artist_shown_as`.
CREATE TABLE IF NOT EXISTS artist_merges (
    id TEXT PRIMARY KEY,
    into_artist_id TEXT NOT NULL,
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
    created_at TEXT NOT NULL,
    -- A new id each time bae writes the image's bytes. It changes on every
    -- device with the write, while the file columns follow only once the
    -- upload completes, so an image's version (§4.1) is this and the hash.
    revision TEXT NOT NULL,
    -- coven writes both columns when the file's upload completes; until then
    -- the row names no file, and only the attaching device holds it.
    hash BLOB,
    location TEXT,
    FOREIGN KEY (id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

-- An album groups releases; each pressing's own facts are on its `releases`
-- row.
CREATE TABLE IF NOT EXISTS albums (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    year INTEGER,
    is_compilation INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
) STRICT;

-- The album's credits, in credit order from position 0. A row is one place in
-- the list (its id derives from the album and the position), so a list that
-- names one artist twice keeps both credits. Positions need not be dense: two
-- devices editing one list at once can leave a gap, and readers order by
-- position. `credit_key` names one of the artist's other printed names
-- (`artist_credits`); NULL prints the artist's own name. `join_phrase` is
-- printed after the credit when another follows; NULL prints ", ".
CREATE TABLE IF NOT EXISTS album_artists (
    id TEXT PRIMARY KEY,
    album_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    credit_key TEXT,
    join_phrase TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
    created_at TEXT NOT NULL,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id, credit_key) REFERENCES artist_credits (artist_id, name_key),
    UNIQUE(album_id, position)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_album_artists_credit ON album_artists (artist_id, credit_key);

CREATE INDEX IF NOT EXISTS idx_album_artists_album_id ON album_artists (album_id);

CREATE INDEX IF NOT EXISTS idx_album_artists_artist_id ON album_artists (artist_id);

-- The compositions tracks perform, as MusicBrainz names them.
CREATE TABLE IF NOT EXISTS works (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    disambiguation TEXT,
    work_type TEXT,
    musicbrainz_work_id TEXT NOT NULL,
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
    -- Where the release stands among its album's: the album's primary release
    -- — the one whose cover the album shows and that plays by default — is its
    -- lowest, ties broken by release id. A release joining an album is ordered
    -- after the album's others; making one primary orders it below the
    -- album's lowest.
    album_order REAL NOT NULL,
    release_name TEXT,
    year INTEGER,
    -- Every label the pressing is on, in its source's order: a JSON array of
    -- {"name", "catalog_number"} objects, each stating one or both.
    labels             TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(labels) AND json_type(labels) = 'array'),
    -- Every barcode the pressing is printed with, in its source's order: a
    -- JSON array of strings.
    barcodes           TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(barcodes) AND json_type(barcodes) = 'array'),
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
    source_folder_name TEXT,
    -- EBU R128 integrated loudness over all tracks, in LUFS, measured at import;
    -- NULL when measuring failed. Playback derives the gain from it.
    album_loudness_lufs REAL,
    -- True peak as a linear ratio (1.0 = 0 dBTP), the max over all tracks;
    -- playback caps the gain at 1.0/peak.
    album_peak_linear REAL,
    created_at TEXT NOT NULL,
    CHECK (country IS NULL OR region IS NULL),
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_releases_album_id ON releases (album_id, album_order, id);

-- One stored cover per release; `id` is the release's id.
CREATE TABLE IF NOT EXISTS covers (
    id TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    source_url TEXT,
    created_at TEXT NOT NULL,
    -- A new id each time bae writes the image's bytes. It changes on every
    -- device with the write, while the file columns follow only once the
    -- upload completes, so an image's version (§4.1) is this and the hash.
    revision TEXT NOT NULL,
    -- coven writes both columns when the file's upload completes; until then
    -- the row names no file, and only the attaching device holds it.
    hash BLOB,
    location TEXT,
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
    created_at TEXT NOT NULL,
    -- coven writes both columns when this file's upload completes; until
    -- then the row names no file, and only the attaching device holds it.
    hash BLOB,
    location TEXT,
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

-- The original of each release file this device imported, or the copy a
-- detach made: its path and the modification time recorded then. These paths
-- never leave this device. A row stays through an upload: until the row names
-- a stored file, this device plays the file from here.
CREATE TABLE IF NOT EXISTS local_release_files (
    file_id TEXT NOT NULL PRIMARY KEY REFERENCES release_files(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    modified_at TEXT NOT NULL
) STRICT;

-- The folder hash (SHA-256 over sorted relative paths, sizes and modification
-- times) of the files this device imported each release from. Another device's
-- folders mean nothing here, so it never syncs. A folder whose hash is here is
-- Done in the import list wherever it sits now, and one hash is one release on
-- this device.
CREATE TABLE IF NOT EXISTS local_release_origin (
    release_id   TEXT NOT NULL PRIMARY KEY REFERENCES releases(id) ON DELETE CASCADE,
    content_hash TEXT NOT NULL UNIQUE
) STRICT;


-- Who is credited on a release, and in what role, as the catalog stated it.
CREATE TABLE IF NOT EXISTS release_artist_roles (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    source_credit TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    UNIQUE(release_id, artist_id, position, source)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_artist_roles_release ON release_artist_roles(release_id);

CREATE INDEX IF NOT EXISTS idx_release_artist_roles_artist ON release_artist_roles(artist_id);

-- The catalog entries describing a release, one per catalog, each naming the
-- pressing or its album.
CREATE TABLE IF NOT EXISTS release_records (
    id          TEXT NOT NULL PRIMARY KEY,
    release_id  TEXT NOT NULL,
    catalog     TEXT NOT NULL,
    key         TEXT NOT NULL,
    album_key   TEXT,
    url         TEXT NOT NULL CHECK (url <> ''),
    created_at  TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('pressing', 'album')),
    CHECK (kind = 'pressing' OR album_key IS NULL),
    UNIQUE (release_id, catalog),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_records_catalog_key ON release_records (catalog, key) WHERE kind = 'pressing';

CREATE INDEX IF NOT EXISTS idx_release_records_catalog_album ON release_records
    (catalog, CASE WHEN kind = 'album' THEN key ELSE album_key END);

-- The mediums a release holds — the discs, records or tapes its audio was
-- read from — in order. The id derives from (release, position), so two
-- devices that write one release's mediums write the same rows.
CREATE TABLE IF NOT EXISTS release_mediums (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    -- Counted from one.
    position INTEGER NOT NULL CHECK (position >= 1),
    -- The carrier (a `crate::pressing::Medium` key), where the source states one.
    medium TEXT CHECK (medium IS NULL OR medium <> ''),
    -- A CD is one side; a record or a cassette as many as its tracks name.
    side_count INTEGER NOT NULL CHECK (side_count >= 1),
    created_at TEXT NOT NULL,
    UNIQUE (release_id, position),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

-- The tracks of a release, in playing order.
CREATE TABLE IF NOT EXISTS tracks (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    title TEXT NOT NULL,
    -- The medium the track sits on, one of its release's.
    medium_id TEXT NOT NULL,
    -- The side of that medium, counted from one.
    side INTEGER NOT NULL CHECK (side >= 1),
    -- The track's number on that side.
    track_number INTEGER NOT NULL CHECK (track_number >= 1),
    -- How long the track's audio plays, measured when it was imported.
    duration_ms INTEGER NOT NULL CHECK (duration_ms >= 0),
    created_at TEXT NOT NULL,
    -- Playing order within the release, from zero across all sides.
    position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE,
    FOREIGN KEY (medium_id) REFERENCES release_mediums (id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_tracks_medium_id ON tracks (medium_id);

CREATE INDEX IF NOT EXISTS idx_tracks_release_id ON tracks (release_id);

CREATE UNIQUE INDEX IF NOT EXISTS tracks_release_position ON tracks(release_id, position);

-- A track's own credits, in credit order from position 0, keyed like
-- `album_artists` by (track, position), with the same printed name and join.
-- A track with none takes the album's.
CREATE TABLE IF NOT EXISTS track_artists (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    credit_key TEXT,
    join_phrase TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
    created_at TEXT NOT NULL,
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id, credit_key) REFERENCES artist_credits (artist_id, name_key),
    UNIQUE(track_id, position)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_track_artists_credit ON track_artists (artist_id, credit_key);

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
    -- Sample counts: never negative, whichever device wrote the row.
    pregap_samples INTEGER CHECK (pregap_samples >= 0),
    generated_pregap_samples INTEGER CHECK (generated_pregap_samples >= 0),
    -- A rate and a channel count above zero; a bit depth only for lossless
    -- audio, 1 to 32.
    sample_rate INTEGER NOT NULL CHECK (sample_rate > 0),
    bits_per_sample INTEGER CHECK (bits_per_sample IS NULL OR bits_per_sample BETWEEN 1 AND 32),
    channels INTEGER NOT NULL CHECK (channels > 0),
    -- EBU R128 integrated loudness of the track, in LUFS, measured at import;
    -- NULL when measuring failed or the track is near-silent. Playback derives
    -- the gain from it.
    track_loudness_lufs REAL,
    -- True peak as a linear ratio (1.0 = 0 dBTP), the max over channels;
    -- playback caps the gain at 1.0/peak.
    track_peak_linear REAL,
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
    -- Positions within the file: never negative, whichever device wrote the row.
    start_sample INTEGER NOT NULL CHECK (start_sample >= 0),
    end_sample INTEGER CHECK (end_sample >= 0),
    start_byte INTEGER CHECK (start_byte >= 0),
    end_byte INTEGER CHECK (end_byte >= 0),
    created_at TEXT NOT NULL,
    UNIQUE (audio_format_id, segment_index),
    FOREIGN KEY (audio_format_id) REFERENCES audio_formats (id) ON DELETE CASCADE,
    FOREIGN KEY (file_id) REFERENCES release_files(id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_audio_format_segments_format_id ON audio_format_segments (audio_format_id);

-- The frames in a segment's file that a scrub starts its decode at, one row
-- per segment that has any: each frame's first sample, the byte it begins at,
-- and the first sample a decode from it plays as a decode from the stream's
-- start does. Import writes them for a FLAC the demuxer can only binary-search
-- (no SEEKTABLE, or one with no point past the first sample), whose frames
-- stand alone (exact_from is the frame's own sample): the segment's first
-- frame, then the first frame 10 seconds of audio on, or sooner where the
-- bytes from the last point would outgrow the window a scrub's jump fetches.
-- And for every MP3, whose frames state no position and lean on the frames
-- before them: for the first frame each 10 seconds of audio past the
-- stream's first, the latest frame a decode can start at and still play it
-- exactly.
CREATE TABLE IF NOT EXISTS audio_segment_scrub_points (
    audio_format_segment_id TEXT PRIMARY KEY NOT NULL,
    -- The points in sample order, each three unsigned LEB128 varints: how far
    -- its sample is past the point before's, less one (the first point: its
    -- sample), the same for its byte, and how far exact_from is past its
    -- sample. At least one point, and the last byte ends a varint; a blob
    -- that still doesn't decode to whole points reads as a damaged database.
    points BLOB NOT NULL CHECK (length(points) >= 3 AND hex(substr(points, -1)) < '80'),
    FOREIGN KEY (audio_format_segment_id) REFERENCES audio_format_segments (id) ON DELETE CASCADE
) STRICT;

-- ── What the library lists ────────────────────────────────────────────────────
--
-- Values a list sorts, filters or counts by that come from several library
-- tables, each kept in a device-local table so an index serves it. Local
-- triggers write them in the same write as the rows they come from, on this
-- device or applied from another, so they always match those rows. Coven
-- applies another device's write with foreign keys checked only at its end, so
-- a row can arrive before a row it references: a trigger counts only rows whose
-- references are there, and the referenced row's own trigger, when it arrives,
-- counts the rows that were waiting for it.

-- Each artist and the artist it shows as (§2.3): the survivor of its merges
-- (`merged_artist_survivors`), else itself. Every read of a credit goes
-- through it.
CREATE TABLE IF NOT EXISTS artist_shown_as (
    artist_id TEXT PRIMARY KEY REFERENCES artists (id) ON DELETE CASCADE,
    shown_artist_id TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artist_shown_as_shown ON artist_shown_as (shown_artist_id);

-- Each artist a merge names, and every artist joined to it through merges in
-- either direction: the artists a change to one of those merges can show as
-- another artist.
CREATE VIEW IF NOT EXISTS artist_merge_groups AS
WITH RECURSIVE edge(artist_id, other_id) AS (
    SELECT id, into_artist_id FROM artist_merges
    UNION SELECT into_artist_id, id FROM artist_merges
),
reach(artist_id, member_id) AS (
    SELECT artist_id, artist_id FROM edge
    UNION SELECT reach.artist_id, edge.other_id FROM reach JOIN edge ON edge.artist_id = reach.member_id
)
SELECT artist_id, member_id FROM reach;

-- The composer list (§2.4) counts three things for each artist it shows: the
-- works it is credited on, the releases a track of which performs one of them,
-- and its catalog role credits that none of its works link. Each is a view,
-- kept in a table of its own by the triggers below; the list's counts follow
-- those tables.

-- Each work and each artist shown credited on it.
CREATE VIEW IF NOT EXISTS work_composers AS
SELECT DISTINCT shown.shown_artist_id AS artist_id, wa.work_id
FROM work_artists wa
JOIN artist_shown_as shown ON shown.artist_id = wa.artist_id;

-- Each release and each artist shown credited on a work one of its tracks
-- performs.
CREATE VIEW IF NOT EXISTS release_work_composers AS
SELECT DISTINCT shown.shown_artist_id AS artist_id, t.release_id
FROM tracks t
JOIN track_works tw ON tw.track_id = t.id
JOIN work_artists wa ON wa.work_id = tw.work_id
JOIN artist_shown_as shown ON shown.artist_id = wa.artist_id;

-- Each catalog role credit no work of its artist links, with the artist it
-- shows as and the release it is on: a release's credit when no track of the
-- release performs a work that artist is credited on, a track's when the track
-- performs none.
CREATE VIEW IF NOT EXISTS unlinked_composer_credits AS
SELECT rar.id AS credit_id, credited.shown_artist_id AS artist_id, rar.release_id
FROM release_artist_roles rar
JOIN artist_shown_as credited ON credited.artist_id = rar.artist_id
WHERE NOT EXISTS (
    SELECT 1 FROM tracks t
    JOIN track_works tw ON tw.track_id = t.id
    JOIN work_artists wa ON wa.work_id = tw.work_id
    WHERE t.release_id = rar.release_id
      AND (SELECT shown_artist_id FROM artist_shown_as WHERE artist_id = wa.artist_id)
          = credited.shown_artist_id
)
UNION ALL
SELECT tar.id, credited.shown_artist_id, t.release_id
FROM track_artist_roles tar
JOIN artist_shown_as credited ON credited.artist_id = tar.artist_id
JOIN tracks t ON t.id = tar.track_id
WHERE NOT EXISTS (
    SELECT 1 FROM track_works tw
    JOIN work_artists wa ON wa.work_id = tw.work_id
    WHERE tw.track_id = tar.track_id
      AND (SELECT shown_artist_id FROM artist_shown_as WHERE artist_id = wa.artist_id)
          = credited.shown_artist_id
);

-- The rows of `work_composers`.
CREATE TABLE IF NOT EXISTS composer_work (
    artist_id TEXT NOT NULL,
    work_id TEXT NOT NULL,
    PRIMARY KEY (artist_id, work_id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_composer_work_work ON composer_work (work_id);

-- The rows of `release_work_composers`.
CREATE TABLE IF NOT EXISTS composer_release (
    artist_id TEXT NOT NULL,
    release_id TEXT NOT NULL,
    PRIMARY KEY (artist_id, release_id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_composer_release_release ON composer_release (release_id);

-- The rows of `unlinked_composer_credits`.
CREATE TABLE IF NOT EXISTS composer_unlinked_credit (
    credit_id TEXT PRIMARY KEY,
    artist_id TEXT NOT NULL,
    release_id TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_composer_unlinked_credit_artist ON composer_unlinked_credit (artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_unlinked_credit_release ON composer_unlinked_credit (release_id);

-- Each artist the composer list shows, with its name for the list's order and
-- the three counts: a row while any count is above zero. An index serves each
-- of the list's sorts, ties broken by name, then id.
CREATE TABLE IF NOT EXISTS composer_listing (
    artist_id TEXT PRIMARY KEY REFERENCES artists (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    work_count INTEGER NOT NULL CHECK (work_count >= 0),
    linked_release_count INTEGER NOT NULL CHECK (linked_release_count >= 0),
    unlinked_credit_count INTEGER NOT NULL CHECK (unlinked_credit_count >= 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_composer_listing_name ON composer_listing (name, artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_listing_name_desc ON composer_listing (name DESC, artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_listing_work_count ON composer_listing (work_count, name, artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_listing_work_count_desc ON composer_listing (work_count DESC, name, artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_listing_release_count ON composer_listing (linked_release_count, name, artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_listing_release_count_desc ON composer_listing (linked_release_count DESC, name, artist_id);

-- Each row added to or removed from the three tables counts in or out of its
-- artist's listing row. An artist whose own row has not arrived yet gets its
-- listing row when it does (`artist_added`).
CREATE TRIGGER IF NOT EXISTS composer_work_added AFTER INSERT ON composer_work
BEGIN
    INSERT INTO composer_listing (artist_id, name, work_count, linked_release_count, unlinked_credit_count)
    SELECT id, name, 1, 0, 0 FROM artists WHERE id = NEW.artist_id
    ON CONFLICT (artist_id) DO UPDATE SET work_count = work_count + 1;
END;

CREATE TRIGGER IF NOT EXISTS composer_work_removed AFTER DELETE ON composer_work
BEGIN
    UPDATE composer_listing SET work_count = work_count - 1 WHERE artist_id = OLD.artist_id;
    DELETE FROM composer_listing WHERE artist_id = OLD.artist_id
        AND work_count = 0 AND linked_release_count = 0 AND unlinked_credit_count = 0;
END;

CREATE TRIGGER IF NOT EXISTS composer_release_added AFTER INSERT ON composer_release
BEGIN
    INSERT INTO composer_listing (artist_id, name, work_count, linked_release_count, unlinked_credit_count)
    SELECT id, name, 0, 1, 0 FROM artists WHERE id = NEW.artist_id
    ON CONFLICT (artist_id) DO UPDATE SET linked_release_count = linked_release_count + 1;
END;

CREATE TRIGGER IF NOT EXISTS composer_release_removed AFTER DELETE ON composer_release
BEGIN
    UPDATE composer_listing SET linked_release_count = linked_release_count - 1 WHERE artist_id = OLD.artist_id;
    DELETE FROM composer_listing WHERE artist_id = OLD.artist_id
        AND work_count = 0 AND linked_release_count = 0 AND unlinked_credit_count = 0;
END;

CREATE TRIGGER IF NOT EXISTS composer_unlinked_credit_added AFTER INSERT ON composer_unlinked_credit
BEGIN
    INSERT INTO composer_listing (artist_id, name, work_count, linked_release_count, unlinked_credit_count)
    SELECT id, name, 0, 0, 1 FROM artists WHERE id = NEW.artist_id
    ON CONFLICT (artist_id) DO UPDATE SET unlinked_credit_count = unlinked_credit_count + 1;
END;

CREATE TRIGGER IF NOT EXISTS composer_unlinked_credit_removed AFTER DELETE ON composer_unlinked_credit
BEGIN
    UPDATE composer_listing SET unlinked_credit_count = unlinked_credit_count - 1 WHERE artist_id = OLD.artist_id;
    DELETE FROM composer_listing WHERE artist_id = OLD.artist_id
        AND work_count = 0 AND linked_release_count = 0 AND unlinked_credit_count = 0;
END;

-- Inserting a release id into `composer_release_refresh` rewrites that
-- release's rows of `composer_release` and `composer_unlinked_credit` from
-- their views, adding and removing only the rows that differ; inserting a work
-- id into `composer_work_refresh` does the same for that work's rows of
-- `composer_work`. These two views, and `artist_merge_refresh` below, hold no
-- rows: they name the statements their INSTEAD OF triggers run, which the
-- triggers on the library tables share.
CREATE VIEW IF NOT EXISTS composer_release_refresh AS SELECT NULL AS release_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS composer_release_refresh INSTEAD OF INSERT ON composer_release_refresh
BEGIN
    DELETE FROM composer_release WHERE release_id = NEW.release_id
        AND artist_id NOT IN (SELECT artist_id FROM release_work_composers WHERE release_id = NEW.release_id);
    INSERT OR IGNORE INTO composer_release (artist_id, release_id)
    SELECT artist_id, release_id FROM release_work_composers WHERE release_id = NEW.release_id;
    DELETE FROM composer_unlinked_credit WHERE release_id = NEW.release_id
        AND (credit_id, artist_id) NOT IN (
            SELECT credit_id, artist_id FROM unlinked_composer_credits WHERE release_id = NEW.release_id);
    INSERT OR IGNORE INTO composer_unlinked_credit (credit_id, artist_id, release_id)
    SELECT credit_id, artist_id, release_id FROM unlinked_composer_credits WHERE release_id = NEW.release_id;
END;

CREATE VIEW IF NOT EXISTS composer_work_refresh AS SELECT NULL AS work_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS composer_work_refresh INSTEAD OF INSERT ON composer_work_refresh
BEGIN
    DELETE FROM composer_work WHERE work_id = NEW.work_id
        AND artist_id NOT IN (SELECT artist_id FROM work_composers WHERE work_id = NEW.work_id);
    INSERT OR IGNORE INTO composer_work (artist_id, work_id)
    SELECT artist_id, work_id FROM work_composers WHERE work_id = NEW.work_id;
END;

-- Each work and each release a track of which performs it.
CREATE VIEW IF NOT EXISTS work_releases AS
SELECT DISTINCT tw.work_id, t.release_id FROM track_works tw JOIN tracks t ON t.id = tw.track_id;

-- Each release an artist is credited on: on a work one of its tracks
-- performs, or with a catalog role on the release or one of its tracks.
CREATE VIEW IF NOT EXISTS artist_credited_releases AS
SELECT wa.artist_id, t.release_id
FROM work_artists wa JOIN track_works tw ON tw.work_id = wa.work_id JOIN tracks t ON t.id = tw.track_id
UNION
SELECT artist_id, release_id FROM release_artist_roles
UNION
SELECT tar.artist_id, t.release_id FROM track_artist_roles tar JOIN tracks t ON t.id = tar.track_id;

-- An artist arriving takes its shown artist and a listing row for what was
-- already counted under it, then counts its own credits.
CREATE TRIGGER IF NOT EXISTS artist_added AFTER INSERT ON artists
BEGIN
    INSERT INTO artist_shown_as (artist_id, shown_artist_id)
    VALUES (NEW.id, COALESCE((SELECT survivor_id FROM merged_artist_survivors WHERE artist_id = NEW.id), NEW.id));
    INSERT INTO composer_listing (artist_id, name, work_count, linked_release_count, unlinked_credit_count)
    SELECT NEW.id, NEW.name,
           (SELECT COUNT(*) FROM composer_work WHERE artist_id = NEW.id),
           (SELECT COUNT(*) FROM composer_release WHERE artist_id = NEW.id),
           (SELECT COUNT(*) FROM composer_unlinked_credit WHERE artist_id = NEW.id)
    WHERE EXISTS (SELECT 1 FROM composer_work WHERE artist_id = NEW.id)
       OR EXISTS (SELECT 1 FROM composer_release WHERE artist_id = NEW.id)
       OR EXISTS (SELECT 1 FROM composer_unlinked_credit WHERE artist_id = NEW.id);
    INSERT INTO composer_work_refresh (work_id)
    SELECT DISTINCT work_id FROM work_artists WHERE artist_id = NEW.id;
    INSERT INTO composer_release_refresh (release_id)
    SELECT DISTINCT release_id FROM artist_credited_releases WHERE artist_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS artist_renamed AFTER UPDATE OF name ON artists
BEGIN
    UPDATE composer_listing SET name = NEW.name WHERE artist_id = NEW.id;
END;

-- Inserting a merge's two artists into `artist_merge_refresh` sets the shown
-- artist of every artist joined to either, then rewrites the rows of every work
-- and release those artists are credited on.
CREATE VIEW IF NOT EXISTS artist_merge_refresh AS SELECT NULL AS artist_id, NULL AS into_artist_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS artist_merge_refresh INSTEAD OF INSERT ON artist_merge_refresh
BEGIN
    UPDATE artist_shown_as
    SET shown_artist_id = COALESCE(
        (SELECT survivor_id FROM merged_artist_survivors WHERE artist_id = artist_shown_as.artist_id),
        artist_shown_as.artist_id)
    WHERE artist_id IN (NEW.artist_id, NEW.into_artist_id)
       OR artist_id IN (SELECT member_id FROM artist_merge_groups WHERE artist_id IN (NEW.artist_id, NEW.into_artist_id));
    INSERT INTO composer_work_refresh (work_id)
    SELECT DISTINCT work_id FROM work_artists
    WHERE artist_id IN (NEW.artist_id, NEW.into_artist_id)
       OR artist_id IN (SELECT member_id FROM artist_merge_groups WHERE artist_id IN (NEW.artist_id, NEW.into_artist_id));
    INSERT INTO composer_release_refresh (release_id)
    SELECT DISTINCT release_id FROM artist_credited_releases
    WHERE artist_id IN (NEW.artist_id, NEW.into_artist_id)
       OR artist_id IN (SELECT member_id FROM artist_merge_groups WHERE artist_id IN (NEW.artist_id, NEW.into_artist_id));
END;

CREATE TRIGGER IF NOT EXISTS artist_merge_added AFTER INSERT ON artist_merges
BEGIN
    INSERT INTO artist_merge_refresh (artist_id, into_artist_id) VALUES (NEW.id, NEW.into_artist_id);
END;

CREATE TRIGGER IF NOT EXISTS artist_merge_changed AFTER UPDATE OF id, into_artist_id ON artist_merges
BEGIN
    INSERT INTO artist_merge_refresh (artist_id, into_artist_id) VALUES (OLD.id, OLD.into_artist_id);
    INSERT INTO artist_merge_refresh (artist_id, into_artist_id) VALUES (NEW.id, NEW.into_artist_id);
END;

CREATE TRIGGER IF NOT EXISTS artist_merge_removed AFTER DELETE ON artist_merges
BEGIN
    INSERT INTO artist_merge_refresh (artist_id, into_artist_id) VALUES (OLD.id, OLD.into_artist_id);
END;

-- A work's credited artists change its rows and those of every release
-- performing it.
CREATE TRIGGER IF NOT EXISTS work_artist_added AFTER INSERT ON work_artists
BEGIN
    INSERT INTO composer_work_refresh (work_id) VALUES (NEW.work_id);
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM work_releases WHERE work_id = NEW.work_id;
END;

CREATE TRIGGER IF NOT EXISTS work_artist_changed AFTER UPDATE OF work_id, artist_id ON work_artists
BEGIN
    INSERT INTO composer_work_refresh (work_id) VALUES (OLD.work_id);
    INSERT INTO composer_work_refresh (work_id) VALUES (NEW.work_id);
    INSERT INTO composer_release_refresh (release_id) SELECT DISTINCT release_id FROM work_releases WHERE work_id IN (OLD.work_id, NEW.work_id);
END;

CREATE TRIGGER IF NOT EXISTS work_artist_removed AFTER DELETE ON work_artists
BEGIN
    INSERT INTO composer_work_refresh (work_id) VALUES (OLD.work_id);
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM work_releases WHERE work_id = OLD.work_id;
END;

-- What a track performs changes its release's rows. Deleting a track deletes
-- its links and credits first, which can no longer find its release, so the
-- track's own trigger rewrites the release.
CREATE TRIGGER IF NOT EXISTS track_work_added AFTER INSERT ON track_works
BEGIN
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM tracks WHERE id = NEW.track_id;
END;

CREATE TRIGGER IF NOT EXISTS track_work_changed AFTER UPDATE OF track_id, work_id ON track_works
BEGIN
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM tracks WHERE id = OLD.track_id;
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM tracks WHERE id = NEW.track_id;
END;

CREATE TRIGGER IF NOT EXISTS track_work_removed AFTER DELETE ON track_works
BEGIN
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM tracks WHERE id = OLD.track_id;
END;

-- A track arriving changes its release's rows only when a link or a credit of
-- it arrived first.
CREATE TRIGGER IF NOT EXISTS track_added AFTER INSERT ON tracks
WHEN EXISTS (SELECT 1 FROM track_works WHERE track_id = NEW.id)
  OR EXISTS (SELECT 1 FROM track_artist_roles WHERE track_id = NEW.id)
BEGIN
    INSERT INTO composer_release_refresh (release_id) VALUES (NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS track_moved AFTER UPDATE OF release_id ON tracks
BEGIN
    INSERT INTO composer_release_refresh (release_id) VALUES (OLD.release_id);
    INSERT INTO composer_release_refresh (release_id) VALUES (NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS track_removed AFTER DELETE ON tracks
BEGIN
    INSERT INTO composer_release_refresh (release_id) VALUES (OLD.release_id);
END;

-- Whether a role credit is linked depends on the release's works, which a
-- write of the credit itself doesn't change: only its own row follows.
CREATE TRIGGER IF NOT EXISTS release_role_added AFTER INSERT ON release_artist_roles
BEGIN
    INSERT INTO composer_unlinked_credit (credit_id, artist_id, release_id)
    SELECT credit_id, artist_id, release_id FROM unlinked_composer_credits WHERE credit_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS release_role_changed AFTER UPDATE OF id, release_id, artist_id ON release_artist_roles
BEGIN
    DELETE FROM composer_unlinked_credit WHERE credit_id = OLD.id;
    INSERT INTO composer_unlinked_credit (credit_id, artist_id, release_id)
    SELECT credit_id, artist_id, release_id FROM unlinked_composer_credits WHERE credit_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS release_role_removed AFTER DELETE ON release_artist_roles
BEGIN
    DELETE FROM composer_unlinked_credit WHERE credit_id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS track_role_added AFTER INSERT ON track_artist_roles
BEGIN
    INSERT INTO composer_unlinked_credit (credit_id, artist_id, release_id)
    SELECT credit_id, artist_id, release_id FROM unlinked_composer_credits WHERE credit_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS track_role_changed AFTER UPDATE OF id, track_id, artist_id ON track_artist_roles
BEGIN
    DELETE FROM composer_unlinked_credit WHERE credit_id = OLD.id;
    INSERT INTO composer_unlinked_credit (credit_id, artist_id, release_id)
    SELECT credit_id, artist_id, release_id FROM unlinked_composer_credits WHERE credit_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS track_role_removed AFTER DELETE ON track_artist_roles
BEGIN
    DELETE FROM composer_unlinked_credit WHERE credit_id = OLD.id;
END;

-- The album grid (§2.1) lists each album holding a release, and grouped by
-- artist lists it once under each artist its credits show; the artist list
-- counts each artist's albums (§2.3). These three tables hold what those lists
-- sort and count by, kept by the triggers below.

-- Each album the library shows, with the values its sorts compare. The credit
-- keys sort it by artist: every credit's shown artist's sort name (else name),
-- then their ids, each in credit order and joined by a separator below every
-- printable character, so "A" sorts before "A & B" and "A & B" before
-- "A & C"; NULL for an album with no credits.
CREATE TABLE IF NOT EXISTS album_listing (
    album_id TEXT PRIMARY KEY REFERENCES albums (id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    year INTEGER,
    created_at TEXT NOT NULL,
    credit_sort_names TEXT,
    credit_artist_ids TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS idx_album_listing_title ON album_listing (title COLLATE NOCASE, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_title_desc ON album_listing (title COLLATE NOCASE DESC, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_added ON album_listing (created_at, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_added_desc ON album_listing (created_at DESC, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_year ON album_listing (year IS NULL, year, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_year_desc ON album_listing (year IS NULL DESC, year DESC, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_credits ON album_listing (credit_sort_names COLLATE NOCASE, credit_artist_ids, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_credits_desc ON album_listing (credit_sort_names COLLATE NOCASE DESC, credit_artist_ids DESC, album_id);

-- Each album the library shows and each artist one of its credits shows,
-- once however many credits name that artist.
CREATE TABLE IF NOT EXISTS album_artist_link (
    artist_id TEXT NOT NULL,
    album_id TEXT NOT NULL REFERENCES albums (id) ON DELETE CASCADE,
    PRIMARY KEY (artist_id, album_id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_album_artist_link_album ON album_artist_link (album_id);

-- Each artist the artist list shows: one an album the library shows credits,
-- with its name for the list's order, its sort name (else name) for the grid
-- grouped by artist, and how many of those albums credit it.
CREATE TABLE IF NOT EXISTS artist_listing (
    artist_id TEXT PRIMARY KEY REFERENCES artists (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    sort_key TEXT NOT NULL,
    album_count INTEGER NOT NULL CHECK (album_count > 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artist_listing_name ON artist_listing (name, artist_id);

CREATE INDEX IF NOT EXISTS idx_artist_listing_name_desc ON artist_listing (name DESC, artist_id);

CREATE INDEX IF NOT EXISTS idx_artist_listing_album_count ON artist_listing (album_count, name, artist_id);

CREATE INDEX IF NOT EXISTS idx_artist_listing_album_count_desc ON artist_listing (album_count DESC, name, artist_id);

-- Both columns run one way, so the index serves the grid grouped by artist in
-- either direction.
CREATE INDEX IF NOT EXISTS idx_artist_listing_sort_key ON artist_listing (sort_key COLLATE NOCASE DESC, artist_id DESC);

-- Each link added to or removed from `album_artist_link` counts in or out of
-- its artist's listing row. An artist whose own row has not arrived yet gets
-- its listing row when it does (`artist_listing_added`).
CREATE TRIGGER IF NOT EXISTS album_artist_link_added AFTER INSERT ON album_artist_link
BEGIN
    INSERT INTO artist_listing (artist_id, name, sort_key, album_count)
    SELECT id, name, COALESCE(sort_name, name), 1 FROM artists WHERE id = NEW.artist_id
    ON CONFLICT (artist_id) DO UPDATE SET album_count = album_count + 1;
END;

CREATE TRIGGER IF NOT EXISTS album_artist_link_removed AFTER DELETE ON album_artist_link
BEGIN
    DELETE FROM artist_listing WHERE artist_id = OLD.artist_id AND album_count = 1;
    UPDATE artist_listing SET album_count = album_count - 1 WHERE artist_id = OLD.artist_id;
END;

-- Inserting an album id into `album_listing_refresh` rewrites that album's
-- row of `album_listing` and its links from the album, its releases and its
-- credits, adding and removing only the links that differ.
CREATE VIEW IF NOT EXISTS album_listing_refresh AS SELECT NULL AS album_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS album_listing_refresh INSTEAD OF INSERT ON album_listing_refresh
BEGIN
    DELETE FROM album_listing WHERE album_id = NEW.album_id
        AND NOT EXISTS (SELECT 1 FROM releases WHERE album_id = NEW.album_id);
    INSERT INTO album_listing (album_id, title, year, created_at, credit_sort_names, credit_artist_ids)
    SELECT a.id, a.title, a.year, a.created_at,
           (SELECT GROUP_CONCAT(COALESCE(credited.sort_name, credited.name), char(31) ORDER BY credit.position)
            FROM album_artists credit
            JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id
            JOIN artists credited ON credited.id = shown.shown_artist_id
            WHERE credit.album_id = a.id),
           (SELECT GROUP_CONCAT(credited.id, char(31) ORDER BY credit.position)
            FROM album_artists credit
            JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id
            JOIN artists credited ON credited.id = shown.shown_artist_id
            WHERE credit.album_id = a.id)
    FROM albums a
    WHERE a.id = NEW.album_id AND EXISTS (SELECT 1 FROM releases WHERE album_id = a.id)
    ON CONFLICT (album_id) DO UPDATE SET
        title = excluded.title,
        year = excluded.year,
        created_at = excluded.created_at,
        credit_sort_names = excluded.credit_sort_names,
        credit_artist_ids = excluded.credit_artist_ids;
    DELETE FROM album_artist_link WHERE album_id = NEW.album_id
        AND artist_id NOT IN (
            SELECT shown.shown_artist_id FROM album_artists credit
            JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id
            WHERE credit.album_id = NEW.album_id
              AND EXISTS (SELECT 1 FROM releases WHERE album_id = NEW.album_id));
    INSERT OR IGNORE INTO album_artist_link (artist_id, album_id)
    SELECT DISTINCT shown.shown_artist_id, credit.album_id FROM album_artists credit
    JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id
    WHERE credit.album_id = NEW.album_id
      AND EXISTS (SELECT 1 FROM releases WHERE album_id = NEW.album_id);
END;

CREATE TRIGGER IF NOT EXISTS album_listing_added AFTER INSERT ON albums
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS album_listing_changed AFTER UPDATE OF title, year, created_at ON albums
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS album_release_added AFTER INSERT ON releases
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.album_id);
END;

CREATE TRIGGER IF NOT EXISTS album_release_moved AFTER UPDATE OF album_id ON releases
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (OLD.album_id);
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.album_id);
END;

CREATE TRIGGER IF NOT EXISTS album_release_removed AFTER DELETE ON releases
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (OLD.album_id);
END;

CREATE TRIGGER IF NOT EXISTS album_credit_added AFTER INSERT ON album_artists
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.album_id);
END;

CREATE TRIGGER IF NOT EXISTS album_credit_changed AFTER UPDATE OF album_id, artist_id, position ON album_artists
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (OLD.album_id);
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.album_id);
END;

CREATE TRIGGER IF NOT EXISTS album_credit_removed AFTER DELETE ON album_artists
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (OLD.album_id);
END;

-- An artist arriving moves every album it is credited on, and every album
-- crediting an artist that shows as it, which waited for its name. An
-- artist's shown artist changing moves every album it is credited on.
CREATE TRIGGER IF NOT EXISTS album_credit_shown_added AFTER INSERT ON artist_shown_as
BEGIN
    INSERT INTO album_listing_refresh (album_id)
    SELECT DISTINCT credit.album_id FROM album_artists credit
    WHERE credit.artist_id = NEW.artist_id
       OR credit.artist_id IN (SELECT artist_id FROM artist_shown_as WHERE shown_artist_id = NEW.artist_id);
END;

CREATE TRIGGER IF NOT EXISTS album_credit_shown_changed AFTER UPDATE OF shown_artist_id ON artist_shown_as
BEGIN
    INSERT INTO album_listing_refresh (album_id)
    SELECT DISTINCT album_id FROM album_artists WHERE artist_id = NEW.artist_id;
END;

-- An artist arriving takes a listing row for the links already counted under
-- it, however many of its own links have counted in by then.
CREATE TRIGGER IF NOT EXISTS artist_listing_added AFTER INSERT ON artists
WHEN EXISTS (SELECT 1 FROM album_artist_link WHERE artist_id = NEW.id)
BEGIN
    INSERT INTO artist_listing (artist_id, name, sort_key, album_count)
    VALUES (NEW.id, NEW.name, COALESCE(NEW.sort_name, NEW.name),
            (SELECT COUNT(*) FROM album_artist_link WHERE artist_id = NEW.id))
    ON CONFLICT (artist_id) DO UPDATE SET album_count = excluded.album_count;
END;

-- A renamed artist moves in the artist list, and so do the albums whose
-- credits show it.
CREATE TRIGGER IF NOT EXISTS artist_listing_renamed AFTER UPDATE OF name, sort_name ON artists
BEGIN
    UPDATE artist_listing SET name = NEW.name, sort_key = COALESCE(NEW.sort_name, NEW.name)
    WHERE artist_id = NEW.id;
    INSERT INTO album_listing_refresh (album_id)
    SELECT DISTINCT credit.album_id FROM artist_shown_as shown
    JOIN album_artists credit ON credit.artist_id = shown.artist_id
    WHERE shown.shown_artist_id = NEW.id;
END;

-- The Storage Manager (§4.2) lists every release with its album's title and
-- credits, its media, its files' count and size and whether they are stored,
-- sorted and filtered by any of them. `release_listing` holds those values,
-- kept by the triggers below, with an index for each sort, alone and after
-- `stored` for the Remote and Local filters.

-- Each release, with what the Storage Manager sorts and filters it by:
-- `album_title` and the credit keys copied from its album's `album_listing`
-- row; `media_empty` (1 for a release stating no medium) and `media_key`
-- (every stated medium with its count, in order); its files' count and total
-- size; and `stored`, 1 when it has files and every one names its stored file.
CREATE TABLE IF NOT EXISTS release_listing (
    release_id TEXT PRIMARY KEY REFERENCES releases (id) ON DELETE CASCADE,
    album_id TEXT NOT NULL,
    album_title TEXT NOT NULL,
    credit_sort_names TEXT,
    credit_artist_ids TEXT,
    media_empty INTEGER NOT NULL,
    media_key TEXT,
    created_at TEXT NOT NULL,
    file_count INTEGER NOT NULL,
    total_size INTEGER NOT NULL,
    stored INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_listing_album ON release_listing (album_id);

CREATE INDEX IF NOT EXISTS idx_release_listing_title ON release_listing (album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_title_desc ON release_listing (album_title COLLATE NOCASE DESC, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_credits ON release_listing (credit_sort_names COLLATE NOCASE, credit_artist_ids, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_credits_desc ON release_listing (credit_sort_names COLLATE NOCASE DESC, credit_artist_ids DESC, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_media ON release_listing (media_empty, media_key, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_media_desc ON release_listing (media_empty, media_key DESC, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_files ON release_listing (file_count, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_files_desc ON release_listing (file_count DESC, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_size ON release_listing (total_size, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_size_desc ON release_listing (total_size DESC, album_title COLLATE NOCASE, created_at, release_id);

CREATE INDEX IF NOT EXISTS idx_release_listing_stored_title ON release_listing (stored, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_title_desc ON release_listing (stored, album_title COLLATE NOCASE DESC, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_credits ON release_listing (stored, credit_sort_names COLLATE NOCASE, credit_artist_ids, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_credits_desc ON release_listing (stored, credit_sort_names COLLATE NOCASE DESC, credit_artist_ids DESC, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_media ON release_listing (stored, media_empty, media_key, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_media_desc ON release_listing (stored, media_empty, media_key DESC, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_files ON release_listing (stored, file_count, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_files_desc ON release_listing (stored, file_count DESC, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_size ON release_listing (stored, total_size, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_size_desc ON release_listing (stored, total_size DESC, album_title COLLATE NOCASE, created_at, release_id);

-- How many releases are stored and how many aren't, and their files' total
-- size, for the Storage Manager's count and total: a row for each of `stored`
-- 0 and 1 while any release has it.
CREATE TABLE IF NOT EXISTS release_listing_totals (
    stored INTEGER PRIMARY KEY,
    release_count INTEGER NOT NULL CHECK (release_count > 0),
    total_size INTEGER NOT NULL
) STRICT;

CREATE TRIGGER IF NOT EXISTS release_listing_added AFTER INSERT ON release_listing
BEGIN
    INSERT INTO release_listing_totals (stored, release_count, total_size)
    VALUES (NEW.stored, 1, NEW.total_size)
    ON CONFLICT (stored) DO UPDATE SET
        release_count = release_count + 1, total_size = total_size + excluded.total_size;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_changed AFTER UPDATE OF stored, total_size ON release_listing
BEGIN
    DELETE FROM release_listing_totals WHERE stored = OLD.stored AND release_count = 1;
    UPDATE release_listing_totals SET release_count = release_count - 1, total_size = total_size - OLD.total_size
    WHERE stored = OLD.stored;
    INSERT INTO release_listing_totals (stored, release_count, total_size)
    VALUES (NEW.stored, 1, NEW.total_size)
    ON CONFLICT (stored) DO UPDATE SET
        release_count = release_count + 1, total_size = total_size + excluded.total_size;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_removed AFTER DELETE ON release_listing
BEGIN
    DELETE FROM release_listing_totals WHERE stored = OLD.stored AND release_count = 1;
    UPDATE release_listing_totals SET release_count = release_count - 1, total_size = total_size - OLD.total_size
    WHERE stored = OLD.stored;
END;

-- Inserting a release id into `release_listing_refresh` rewrites that
-- release's row of `release_listing` from the release, its files and its
-- album's `album_listing` row; a release whose album is not listed yet gets
-- its row when the album's is written.
CREATE VIEW IF NOT EXISTS release_listing_refresh AS SELECT NULL AS release_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS release_listing_refresh INSTEAD OF INSERT ON release_listing_refresh
BEGIN
    INSERT INTO release_listing (
        release_id, album_id, album_title, credit_sort_names, credit_artist_ids,
        media_empty, media_key, created_at, file_count, total_size, stored)
    SELECT r.id, r.album_id, album.title, album.credit_sort_names, album.credit_artist_ids,
           json_array_length(r.media) = 0,
           (SELECT group_concat(printf('%s:%05d', json_extract(entry.value, '$.medium'),
                json_extract(entry.value, '$.count')), ',')
            FROM json_each(r.media) AS entry),
           r.created_at,
           (SELECT COUNT(*) FROM release_files f WHERE f.release_id = r.id),
           (SELECT COALESCE(SUM(f.file_size), 0) FROM release_files f WHERE f.release_id = r.id),
           EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = r.id)
             AND NOT EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = r.id AND f.hash IS NULL)
    FROM releases r
    JOIN album_listing album ON album.album_id = r.album_id
    WHERE r.id = NEW.release_id
    ON CONFLICT (release_id) DO UPDATE SET
        album_id = excluded.album_id,
        album_title = excluded.album_title,
        credit_sort_names = excluded.credit_sort_names,
        credit_artist_ids = excluded.credit_artist_ids,
        media_empty = excluded.media_empty,
        media_key = excluded.media_key,
        created_at = excluded.created_at,
        file_count = excluded.file_count,
        total_size = excluded.total_size,
        stored = excluded.stored;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_release_added AFTER INSERT ON releases
BEGIN
    INSERT INTO release_listing_refresh (release_id) VALUES (NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS release_listing_release_changed AFTER UPDATE OF album_id, media, created_at ON releases
BEGIN
    INSERT INTO release_listing_refresh (release_id) VALUES (NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS release_listing_file_added AFTER INSERT ON release_files
BEGIN
    INSERT INTO release_listing_refresh (release_id) VALUES (NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS release_listing_file_changed AFTER UPDATE OF release_id, file_size, hash ON release_files
BEGIN
    INSERT INTO release_listing_refresh (release_id) VALUES (OLD.release_id);
    INSERT INTO release_listing_refresh (release_id) VALUES (NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS release_listing_file_removed AFTER DELETE ON release_files
BEGIN
    INSERT INTO release_listing_refresh (release_id) VALUES (OLD.release_id);
END;

CREATE TRIGGER IF NOT EXISTS release_listing_album_listed AFTER INSERT ON album_listing
BEGIN
    INSERT INTO release_listing_refresh (release_id)
    SELECT id FROM releases WHERE album_id = NEW.album_id;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_album_changed AFTER UPDATE OF title, credit_sort_names, credit_artist_ids ON album_listing
BEGIN
    INSERT INTO release_listing_refresh (release_id)
    SELECT id FROM releases WHERE album_id = NEW.album_id;
END;

-- A release file's key as coven's state views name it (`coven_key`), so a
-- read joining coven's upload queue or pins reaches the file by its key.
CREATE INDEX IF NOT EXISTS idx_release_files_coven_key ON release_files (coven_key(id));

-- Library search (§2.3) matches a query anywhere inside a title or a name,
-- ignoring case: each searched text is kept in an FTS5 table as
-- `bae_fold_case` folds it, which is how the query is folded too, indexed by
-- its runs of three characters, so a query of three or more finds its rows
-- without reading every title; a shorter one reads the whole table. Each FTS5
-- row also carries, unindexed, the key of the row it searches and what that
-- kind of result is ordered by, so a search orders its matches without
-- reading the rows they stand for. An FTS5 row is found by an integer, which
-- each `_entry` table pairs with the searched row's key for the triggers that
-- keep it: SQLite's own rowid of a synced row is no part of it.

-- Album titles.
CREATE TABLE IF NOT EXISTS album_search_entry (
    entry INTEGER PRIMARY KEY,
    album_id TEXT NOT NULL,
    UNIQUE (album_id)
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS album_search USING fts5(
    title, album_id UNINDEXED,
    tokenize = 'trigram case_sensitive 1'
);

CREATE TRIGGER IF NOT EXISTS album_search_added AFTER INSERT ON albums
BEGIN
    INSERT INTO album_search_entry (album_id) VALUES (NEW.id);
    INSERT INTO album_search (rowid, title, album_id)
    VALUES ((SELECT entry FROM album_search_entry WHERE album_id = NEW.id), bae_fold_case(NEW.title), NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS album_search_changed AFTER UPDATE OF id, title ON albums
BEGIN
    UPDATE album_search_entry SET album_id = NEW.id WHERE album_id = OLD.id;
    UPDATE album_search SET title = bae_fold_case(NEW.title), album_id = NEW.id WHERE rowid = (SELECT entry FROM album_search_entry WHERE album_id = NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS album_search_removed AFTER DELETE ON albums
BEGIN
    DELETE FROM album_search WHERE rowid = (SELECT entry FROM album_search_entry WHERE album_id = OLD.id);
    DELETE FROM album_search_entry WHERE album_id = OLD.id;
END;

-- Track titles, each with its own title to order by.
CREATE TABLE IF NOT EXISTS track_search_entry (
    entry INTEGER PRIMARY KEY,
    track_id TEXT NOT NULL,
    UNIQUE (track_id)
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS track_search USING fts5(
    title, track_id UNINDEXED, sort_title UNINDEXED,
    tokenize = 'trigram case_sensitive 1'
);

CREATE TRIGGER IF NOT EXISTS track_search_added AFTER INSERT ON tracks
BEGIN
    INSERT INTO track_search_entry (track_id) VALUES (NEW.id);
    INSERT INTO track_search (rowid, title, track_id, sort_title)
    VALUES ((SELECT entry FROM track_search_entry WHERE track_id = NEW.id), bae_fold_case(NEW.title), NEW.id, NEW.title);
END;

CREATE TRIGGER IF NOT EXISTS track_search_changed AFTER UPDATE OF id, title ON tracks
BEGIN
    UPDATE track_search_entry SET track_id = NEW.id WHERE track_id = OLD.id;
    UPDATE track_search SET title = bae_fold_case(NEW.title), track_id = NEW.id, sort_title = NEW.title WHERE rowid = (SELECT entry FROM track_search_entry WHERE track_id = NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS track_search_removed AFTER DELETE ON tracks
BEGIN
    DELETE FROM track_search WHERE rowid = (SELECT entry FROM track_search_entry WHERE track_id = OLD.id);
    DELETE FROM track_search_entry WHERE track_id = OLD.id;
END;

-- Work titles, each with its own title to order by.
CREATE TABLE IF NOT EXISTS work_search_entry (
    entry INTEGER PRIMARY KEY,
    work_id TEXT NOT NULL,
    UNIQUE (work_id)
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS work_search USING fts5(
    title, work_id UNINDEXED, sort_title UNINDEXED,
    tokenize = 'trigram case_sensitive 1'
);

CREATE TRIGGER IF NOT EXISTS work_search_added AFTER INSERT ON works
BEGIN
    INSERT INTO work_search_entry (work_id) VALUES (NEW.id);
    INSERT INTO work_search (rowid, title, work_id, sort_title)
    VALUES ((SELECT entry FROM work_search_entry WHERE work_id = NEW.id), bae_fold_case(NEW.title), NEW.id, NEW.title);
END;

CREATE TRIGGER IF NOT EXISTS work_search_changed AFTER UPDATE OF id, title ON works
BEGIN
    UPDATE work_search_entry SET work_id = NEW.id WHERE work_id = OLD.id;
    UPDATE work_search SET title = bae_fold_case(NEW.title), work_id = NEW.id, sort_title = NEW.title WHERE rowid = (SELECT entry FROM work_search_entry WHERE work_id = NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS work_search_removed AFTER DELETE ON works
BEGIN
    DELETE FROM work_search WHERE rowid = (SELECT entry FROM work_search_entry WHERE work_id = OLD.id);
    DELETE FROM work_search_entry WHERE work_id = OLD.id;
END;

-- Artists' names and sort names.
CREATE TABLE IF NOT EXISTS artist_search_entry (
    entry INTEGER PRIMARY KEY,
    artist_id TEXT NOT NULL,
    UNIQUE (artist_id)
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS artist_search USING fts5(
    name, sort_name, artist_id UNINDEXED,
    tokenize = 'trigram case_sensitive 1'
);

CREATE TRIGGER IF NOT EXISTS artist_search_added AFTER INSERT ON artists
BEGIN
    INSERT INTO artist_search_entry (artist_id) VALUES (NEW.id);
    INSERT INTO artist_search (rowid, name, sort_name, artist_id)
    VALUES ((SELECT entry FROM artist_search_entry WHERE artist_id = NEW.id), bae_fold_case(NEW.name), bae_fold_case(NEW.sort_name), NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS artist_search_changed AFTER UPDATE OF id, name, sort_name ON artists
BEGIN
    UPDATE artist_search_entry SET artist_id = NEW.id WHERE artist_id = OLD.id;
    UPDATE artist_search SET name = bae_fold_case(NEW.name), sort_name = bae_fold_case(NEW.sort_name), artist_id = NEW.id WHERE rowid = (SELECT entry FROM artist_search_entry WHERE artist_id = NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS artist_search_removed AFTER DELETE ON artists
BEGIN
    DELETE FROM artist_search WHERE rowid = (SELECT entry FROM artist_search_entry WHERE artist_id = OLD.id);
    DELETE FROM artist_search_entry WHERE artist_id = OLD.id;
END;

-- Artists' other printed names.
CREATE TABLE IF NOT EXISTS artist_credit_search_entry (
    entry INTEGER PRIMARY KEY,
    artist_id TEXT NOT NULL,
    name_key TEXT NOT NULL,
    UNIQUE (artist_id, name_key)
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS artist_credit_search USING fts5(
    name, artist_id UNINDEXED, name_key UNINDEXED,
    tokenize = 'trigram case_sensitive 1'
);

CREATE TRIGGER IF NOT EXISTS artist_credit_search_added AFTER INSERT ON artist_credits
BEGIN
    INSERT INTO artist_credit_search_entry (artist_id, name_key) VALUES (NEW.artist_id, NEW.name_key);
    INSERT INTO artist_credit_search (rowid, name, artist_id, name_key)
    VALUES ((SELECT entry FROM artist_credit_search_entry WHERE artist_id = NEW.artist_id AND name_key = NEW.name_key), bae_fold_case(NEW.name), NEW.artist_id, NEW.name_key);
END;

CREATE TRIGGER IF NOT EXISTS artist_credit_search_changed AFTER UPDATE OF artist_id, name_key, name ON artist_credits
BEGIN
    UPDATE artist_credit_search_entry SET artist_id = NEW.artist_id, name_key = NEW.name_key WHERE artist_id = OLD.artist_id AND name_key = OLD.name_key;
    UPDATE artist_credit_search SET name = bae_fold_case(NEW.name), artist_id = NEW.artist_id, name_key = NEW.name_key WHERE rowid = (SELECT entry FROM artist_credit_search_entry WHERE artist_id = NEW.artist_id AND name_key = NEW.name_key);
END;

CREATE TRIGGER IF NOT EXISTS artist_credit_search_removed AFTER DELETE ON artist_credits
BEGIN
    DELETE FROM artist_credit_search WHERE rowid = (SELECT entry FROM artist_credit_search_entry WHERE artist_id = OLD.artist_id AND name_key = OLD.name_key);
    DELETE FROM artist_credit_search_entry WHERE artist_id = OLD.artist_id AND name_key = OLD.name_key;
END;

-- ── Playback ──────────────────────────────────────────────────────────────────

-- What this device plays next, and how. One row; never synced. Up Next is
-- `playback_manual`, the context lane `playback_context`.
CREATE TABLE IF NOT EXISTS playback_queue (
    singleton  INTEGER PRIMARY KEY CHECK (singleton = 1),
    repeat     TEXT NOT NULL CHECK (repeat IN ('off', 'track', 'context')),
    -- The entry number the next Up Next entry gets: no two entries ever share
    -- one.
    next_entry INTEGER NOT NULL,
    -- What the context lane plays from (a release id, a JSON array of release
    -- ids, or 'library'), whether it plays shuffled, and the position of its
    -- track playing or last played: the lane's upcoming tracks are the ones
    -- after it. NULL together exactly while the lane is empty.
    source     TEXT,
    shuffled   INTEGER CHECK (shuffled IN (0, 1)),
    cursor     INTEGER,
    CHECK ((source IS NULL) = (shuffled IS NULL) AND (source IS NULL) = (cursor IS NULL))
) STRICT;

INSERT OR IGNORE INTO playback_queue (singleton, repeat, next_entry)
VALUES (1, 'off', 1);

-- Up Next: tracks queued by hand, drained first, in position order. Each is an
-- entry of its own, so a track queued twice is two rows. Positions are only an
-- order: the lane has gaps where entries left it. Never synced.
CREATE TABLE IF NOT EXISTS playback_manual (
    entry    INTEGER PRIMARY KEY,
    position INTEGER NOT NULL UNIQUE,
    track_id TEXT NOT NULL
) STRICT;
-- A deleted track's entries are found by its id.
CREATE INDEX IF NOT EXISTS playback_manual_track ON playback_manual(track_id);

-- The context lane a play fills, keyed by position in play order, so the lane
-- is stored the way it plays and grows at its end as a play fills it. A lane
-- never holds a track twice, so a row is named by its track. Positions are
-- only an order, with gaps where rows left. Never synced.
CREATE TABLE IF NOT EXISTS playback_context (
    position   INTEGER PRIMARY KEY,
    track_id   TEXT NOT NULL UNIQUE,
    -- The row's position in the order unshuffling puts the lane back in: its
    -- order when shuffle turned on, or the source's order for a shuffled play.
    -- Read only while the lane is shuffled.
    unshuffled INTEGER NOT NULL
) STRICT;

-- The context lane's last row leaving takes what it played from with it.
CREATE TRIGGER IF NOT EXISTS playback_context_emptied AFTER DELETE ON playback_context
WHEN NOT EXISTS (SELECT 1 FROM playback_context)
BEGIN
    UPDATE playback_queue SET source = NULL, shuffled = NULL, cursor = NULL;
END;

-- A deleted track leaves the queue in the same transaction. A trigger rather
-- than a foreign key: the queue's rows are only ever made from tracks the
-- library holds, so a key would only look each one up again as a play fills
-- the lane.
CREATE TRIGGER IF NOT EXISTS tracks_leave_playback_queue AFTER DELETE ON tracks
BEGIN
    DELETE FROM playback_manual WHERE track_id = OLD.id;
    DELETE FROM playback_context WHERE track_id = OLD.id;
END;

-- Where this device's playback stood, so a restart resumes it: the queue
-- entry playing, how far into its stream, and the volume. Present while a
-- track is loaded; never synced.
CREATE TABLE IF NOT EXISTS playback_state (
    id               TEXT PRIMARY KEY,
    -- The entry id the queue screens show ('m' and an Up Next entry number,
    -- or 'c' and a context track). Read only with its track: a deleted track
    -- leaves nothing to resume.
    current_entry    TEXT,
    current_track_id TEXT REFERENCES tracks(id) ON DELETE SET NULL,
    position_ms      INTEGER CHECK (position_ms >= 0),
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
-- where it has not is newly found. Only scans write it: a release built from
-- folders picked together is dated by theirs.
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
    -- 'grouping' for a release a grouping with no anchor builds from its
    -- members.
    source_kind                    TEXT NOT NULL DEFAULT 'folder' CHECK (source_kind IN ('folder', 'grouping')),
    PRIMARY KEY (watched_folder_path, path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE,
    -- A release a grouping reads as one leaves the queue with its grouping.
    -- Checked as the write commits, since a write stores the two in either
    -- order.
    FOREIGN KEY (grouping_key) REFERENCES release_grouping (key)
        ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
    CHECK ((kind = 'invalid') = (content_hash IS NULL)),
    CHECK ((source_kind = 'grouping') <= (grouping_key IS NOT NULL))
) STRICT;

CREATE INDEX IF NOT EXISTS idx_scan_candidate_path ON scan_candidate (path);

-- The release a grouping reads as one, which goes with the grouping.
CREATE INDEX IF NOT EXISTS idx_scan_candidate_grouping ON scan_candidate (grouping_key);

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
    role                  TEXT NOT NULL CHECK (role IN ('audio', 'track_sheet', 'artwork', 'damaged_image', 'document', 'other')),
    sheet_binding         TEXT CHECK (sheet_binding IS NULL OR sheet_binding IN ('resolved', 'unresolved', 'refused_timing', 'refused_codec')),
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
    -- Every value a multi-valued tag states, in order, as a JSON array of
    -- strings: the track's artists, the album's artists, the ISRCs the file's
    -- recording is registered under, its labels and copyright lines.
    track_artists       TEXT NOT NULL
        CHECK (json_valid(track_artists) AND json_type(track_artists) = 'array'),
    album_title         TEXT,
    album_artists       TEXT NOT NULL
        CHECK (json_valid(album_artists) AND json_type(album_artists) = 'array'),
    -- Whether the tags flag the file part of a compilation.
    compilation         INTEGER NOT NULL CHECK (compilation IN (0, 1)),
    -- Every year the file's dates name, each once, as a JSON array of integers.
    years               TEXT NOT NULL
        CHECK (json_valid(years) AND json_type(years) = 'array'),
    track_number        INTEGER,
    disc_number         INTEGER,
    isrcs               TEXT NOT NULL
        CHECK (json_valid(isrcs) AND json_type(isrcs) = 'array'),
    labels              TEXT NOT NULL
        CHECK (json_valid(labels) AND json_type(labels) = 'array'),
    copyrights          TEXT NOT NULL
        CHECK (json_valid(copyrights) AND json_type(copyrights) = 'array'),
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

-- Every file of a release that failed validation, in name order, as the scan
-- read it: by its name, and for audio and images by its bytes. Its damaged
-- audio is why it failed, and how many is the reason the Skipped tab gives.
CREATE TABLE IF NOT EXISTS scan_skipped_file (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    relative_path       TEXT NOT NULL,
    absolute_path       TEXT NOT NULL,
    size                INTEGER NOT NULL CHECK (size >= 0),
    modified_at_ns      INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    role                TEXT NOT NULL CHECK (role IN ('audio', 'damaged_audio', 'track_sheet', 'artwork', 'damaged_image', 'document', 'other')),
    -- Why a damaged track can't be played: it holds no bytes, or no stream bae plays.
    audio_damage        TEXT CHECK (audio_damage IS NULL OR audio_damage IN ('empty', 'unreadable')),
    PRIMARY KEY (watched_folder_path, candidate_path, position),
    UNIQUE (watched_folder_path, candidate_path, relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE,
    CHECK ((role = 'damaged_audio') = (audio_damage IS NOT NULL))
) STRICT;

-- What the tags of a release that failed validation call it: the album title
-- the first of its playable files to state one gives, with that file's
-- artist. No row when none states one, and for a grouping's release.
CREATE TABLE IF NOT EXISTS scan_skipped_tags (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    album_title         TEXT NOT NULL,
    artist              TEXT,
    PRIMARY KEY (watched_folder_path, candidate_path),
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
    -- 'invalid' when a zero-byte track is among its files, so a release taking
    -- them in cannot be imported; 'downloading' while a download into the
    -- folder runs, when none of its files is stored.
    state               TEXT NOT NULL CHECK (state IN ('valid', 'invalid', 'downloading')),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_scan_sidecar_root ON scan_sidecar (watched_folder_path);

-- One file of a sidecar, in release file order, with its role. Only an
-- invalid sidecar holds a damaged track.
CREATE TABLE IF NOT EXISTS scan_sidecar_file (
    folder              TEXT NOT NULL REFERENCES scan_sidecar (folder) ON DELETE CASCADE,
    position            INTEGER NOT NULL CHECK (position >= 0),
    relative_path       TEXT NOT NULL,
    absolute_path       TEXT NOT NULL UNIQUE,
    size                INTEGER NOT NULL CHECK (size >= 0),
    modified_at_ns      INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    role                TEXT NOT NULL CHECK (role IN ('artwork', 'damaged_image', 'document', 'other', 'damaged_audio')),
    audio_damage        TEXT CHECK (audio_damage IS NULL OR audio_damage IN ('empty', 'unreadable')),
    PRIMARY KEY (folder, position),
    UNIQUE (folder, relative_path),
    CHECK ((role = 'damaged_audio') = (audio_damage IS NOT NULL))
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
    -- The barcode rows as the form holds them: a JSON array of strings, each
    -- as typed.
    barcodes       TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(barcodes) AND json_type(barcodes) = 'array'),
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
    -- A picked artist's printed name: one of its `artist_credits`, NULL for
    -- its own name.
    credit_key            TEXT,
    -- A credit's printed name, where it is not `name`.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    join_phrase           TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    CHECK (
        (assignment_kind = 'picked' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL
            AND credited_as IS NULL)
        OR
        (assignment_kind = 'credit' AND artist_id IS NULL AND name IS NOT NULL AND name <> ''
            AND credit_key IS NULL)
    )
) STRICT;

-- The mediums of the draft, in order: what applying a source read the
-- folder's audio to be on.
CREATE TABLE IF NOT EXISTS import_candidate_medium (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL CHECK (position >= 1),
    medium       TEXT CHECK (medium IS NULL OR medium <> ''),
    side_count   INTEGER NOT NULL CHECK (side_count >= 1),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE
) STRICT;

-- The tracks of the draft, each bound to the file (or CUE slice) it plays and
-- placed on a side of one of the draft's mediums. `stated_length_ms` is the
-- length the source the draft was read from lists for the track, unstated
-- where it lists none or the draft was read from no catalog.
CREATE TABLE IF NOT EXISTS import_candidate_track (
    content_hash           TEXT NOT NULL,
    track_id               TEXT NOT NULL,
    position               INTEGER NOT NULL CHECK (position >= 0),
    title                  TEXT NOT NULL,
    artist_assignment_kind TEXT NOT NULL CHECK (artist_assignment_kind IN ('album_artists', 'explicit')),
    medium                 INTEGER NOT NULL CHECK (medium >= 1),
    side                   INTEGER NOT NULL CHECK (side >= 1),
    track_number           INTEGER NOT NULL CHECK (track_number >= 1),
    stated_length_ms       INTEGER CHECK (stated_length_ms IS NULL OR stated_length_ms >= 0),
    file_kind              TEXT NOT NULL CHECK (file_kind IN ('standalone', 'sheet_slice')),
    file_id                TEXT NOT NULL,
    sheet_id               TEXT,
    slice_index            INTEGER CHECK (slice_index IS NULL OR slice_index >= 0),
    PRIMARY KEY (content_hash, track_id),
    UNIQUE (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (content_hash, medium) REFERENCES import_candidate_medium (content_hash, position),
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
    -- A picked artist's printed name: one of its `artist_credits`, NULL for
    -- its own name.
    credit_key            TEXT,
    -- A credit's printed name, where it is not `name`.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    join_phrase           TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
    PRIMARY KEY (content_hash, track_id, position),
    FOREIGN KEY (content_hash, track_id)
        REFERENCES import_candidate_track (content_hash, track_id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    CHECK (
        (assignment_kind = 'picked' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL
            AND credited_as IS NULL)
        OR
        (assignment_kind = 'credit' AND artist_id IS NULL AND name IS NOT NULL AND name <> ''
            AND credit_key IS NULL)
    )
) STRICT;

-- Who the source credits on a draft track in a role, as it stated the role
-- (composers), in its order.
CREATE TABLE IF NOT EXISTS import_candidate_track_role (
    content_hash          TEXT NOT NULL,
    track_id              TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL CHECK (name <> ''),
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    source                TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    role                  TEXT,
    PRIMARY KEY (content_hash, track_id, position),
    FOREIGN KEY (content_hash, track_id)
        REFERENCES import_candidate_track (content_hash, track_id) ON DELETE CASCADE
) STRICT;

-- Who the source credits in a role on the release itself rather than on one
-- track (Discogs's release-level composers), in its order.
CREATE TABLE IF NOT EXISTS import_candidate_release_role (
    content_hash          TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL CHECK (name <> ''),
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    source                TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    role                  TEXT,
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE
) STRICT;

-- Every MusicBrainz work the draft's tracks perform, and the works those are
-- parts of or hold as parts, once each, in the order the source reached them.
CREATE TABLE IF NOT EXISTS import_candidate_work (
    content_hash        TEXT NOT NULL,
    musicbrainz_work_id TEXT NOT NULL CHECK (musicbrainz_work_id <> ''),
    position            INTEGER NOT NULL CHECK (position >= 0),
    title               TEXT NOT NULL,
    disambiguation      TEXT,
    work_type           TEXT,
    PRIMARY KEY (content_hash, musicbrainz_work_id),
    UNIQUE (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE
) STRICT;

-- A draft work's composers, in the work's order.
CREATE TABLE IF NOT EXISTS import_candidate_work_composer (
    content_hash          TEXT NOT NULL,
    musicbrainz_work_id   TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL CHECK (name <> ''),
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (content_hash, musicbrainz_work_id, position),
    FOREIGN KEY (content_hash, musicbrainz_work_id)
        REFERENCES import_candidate_work (content_hash, musicbrainz_work_id) ON DELETE CASCADE
) STRICT;

-- A draft work that is part of another, in the order the source relates them.
CREATE TABLE IF NOT EXISTS import_candidate_work_part (
    content_hash     TEXT NOT NULL,
    position         INTEGER NOT NULL CHECK (position >= 0),
    parent_work_id   TEXT NOT NULL,
    child_work_id    TEXT NOT NULL,
    PRIMARY KEY (content_hash, position),
    UNIQUE (content_hash, parent_work_id, child_work_id),
    FOREIGN KEY (content_hash, parent_work_id)
        REFERENCES import_candidate_work (content_hash, musicbrainz_work_id) ON DELETE CASCADE,
    FOREIGN KEY (content_hash, child_work_id)
        REFERENCES import_candidate_work (content_hash, musicbrainz_work_id) ON DELETE CASCADE
) STRICT;

-- The works a draft track performs, in the order it performs them.
CREATE TABLE IF NOT EXISTS import_candidate_track_work (
    content_hash        TEXT NOT NULL,
    track_id            TEXT NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    musicbrainz_work_id TEXT NOT NULL,
    PRIMARY KEY (content_hash, track_id, position),
    UNIQUE (content_hash, track_id, musicbrainz_work_id),
    FOREIGN KEY (content_hash, track_id)
        REFERENCES import_candidate_track (content_hash, track_id) ON DELETE CASCADE,
    FOREIGN KEY (content_hash, musicbrainz_work_id)
        REFERENCES import_candidate_work (content_hash, musicbrainz_work_id) ON DELETE CASCADE
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

-- The library album the person picked for a candidate to join, over the one
-- its records name. No row: the import files under the album the records name,
-- or a new one. An album deleted while picked takes its pick with it.
CREATE TABLE IF NOT EXISTS import_candidate_album_pick (
    content_hash TEXT PRIMARY KEY,
    album_id     TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
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

-- The cover images the catalog release last applied to the draft offers the
-- picker — every claimed release's own images, then their albums' — in the
-- order the picker shows them. The apply writes them; a draft read from the
-- files' tags, typed in, or linked to an album offers none.
CREATE TABLE IF NOT EXISTS import_candidate_cover_option (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL CHECK (position >= 0),
    url          TEXT NOT NULL CHECK (url <> ''),
    label        TEXT NOT NULL,
    source       TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    standing     TEXT NOT NULL CHECK (standing IN ('stated', 'unstated')),
    PRIMARY KEY (content_hash, position),
    UNIQUE (content_hash, url),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The downscaled copies the catalog serves of an offered cover, one per size.
CREATE TABLE IF NOT EXISTS import_candidate_cover_option_copy (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    max_edge     INTEGER NOT NULL CHECK (max_edge > 0),
    url          TEXT NOT NULL CHECK (url <> ''),
    PRIMARY KEY (content_hash, position, max_edge),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_cover_option (content_hash, position) ON DELETE CASCADE
) STRICT;

-- The bytes of a catalog-offered cover, fetched before the import runs.
CREATE TABLE IF NOT EXISTS import_candidate_remote_cover_asset (
    content_hash TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    bytes        BLOB NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_cover (content_hash) ON DELETE CASCADE
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

-- What the candidate is linked to in the catalogs: the pressing picked for its
-- folder ('pressing', naming the release the pick names first), or the album
-- the person could not tell the pressing of ('album', its albums in
-- import_candidate_release_link_album). Independent of the draft: only a
-- pick, identification applying its pick, linking the album, keeping the
-- folder's own draft, or unlinking changes it.
CREATE TABLE IF NOT EXISTS import_candidate_release_link (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('pressing', 'album')),
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    release_id   TEXT,
    -- What the partner and album rows name, so each hangs off a link of its
    -- own kind only.
    UNIQUE (content_hash, kind),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (source, release_id) REFERENCES source_release (catalog, release_id),
    CHECK ((kind = 'pressing') = (source IS NOT NULL)),
    CHECK ((kind = 'pressing') = (release_id IS NOT NULL))
) STRICT;

-- The releases in the other catalogs the linked pressing paired with.
CREATE TABLE IF NOT EXISTS import_candidate_release_link_partner (
    content_hash TEXT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind = 'pressing'),
    source       TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id   TEXT NOT NULL,
    PRIMARY KEY (content_hash, source),
    FOREIGN KEY (content_hash, kind)
        REFERENCES import_candidate_release_link (content_hash, kind) ON DELETE CASCADE,
    FOREIGN KEY (source, release_id) REFERENCES source_release (catalog, release_id)
) STRICT;

-- The linked album in each catalog that files it: its MusicBrainz release
-- group, its Discogs master.
CREATE TABLE IF NOT EXISTS import_candidate_release_link_album (
    content_hash TEXT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind = 'album'),
    catalog      TEXT NOT NULL CHECK (catalog IN ('musicbrainz', 'discogs')),
    album_id     TEXT NOT NULL CHECK (album_id <> ''),
    PRIMARY KEY (content_hash, catalog),
    FOREIGN KEY (content_hash, kind)
        REFERENCES import_candidate_release_link (content_hash, kind) ON DELETE CASCADE
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
        'import', 'choose_artist_identity', 'read_file_tags',
        'change_lookups', 'change_search_words', 'change_agreements', 'keep_own_draft',
        'link_shared_album', 'unlink')),
    error_category TEXT,
    error_detail   TEXT,
    CHECK ((error_command IS NULL) = (error_category IS NULL)),
    CHECK ((error_command IS NULL) = (error_detail IS NULL)),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- Where each candidate something is running for stands right now, keyed as
-- the import list keys it. This table is the only place that holds it: every
-- change is a write its owner awaits, and the guards that decide what may
-- happen next read it in the write they guard.
--
-- Its identification is of the files it names (`content_hash`,
-- `file_edit_revision`, as `scan_candidate` holds them), and ends in the write
-- that leaves the scan holding no candidate with those files, or in the write
-- of a decision about the candidate: queued by an admission, running as
-- `identify_run`, having that run's answer written (the release it picked
-- unattended read first, and whether an Import was asked of that answer), or
-- the answer's write failed. Its import is the one `import_id`
-- names, queued, running or writing its release; a second import is refused
-- while one stands. A row exists only while one of the two is set. No run or
-- import outlives the process, so the table is emptied as the library opens.
-- How far a run or an import has got is not here: it moves too often to be
-- written.
CREATE TABLE IF NOT EXISTS import_candidate_standing (
    candidate_key      TEXT PRIMARY KEY,
    identification     TEXT CHECK (identification IN (
        'queued', 'running', 'finalizing', 'reading_pick', 'not_written', 'inapplicable')),
    content_hash       TEXT,
    file_edit_revision INTEGER CHECK (file_edit_revision >= 0),
    admission          TEXT CHECK (admission IN ('automatic', 'requested')),
    identify_run       INTEGER CHECK (identify_run >= 0),
    import_asked       INTEGER CHECK (import_asked IN (0, 1)),
    save_failure_error TEXT,
    import             TEXT CHECK (import IN ('queued', 'running', 'writing')),
    import_id          TEXT,
    CHECK (identification IS NOT NULL OR import IS NOT NULL),
    CHECK ((identification IS NOT NULL) = (content_hash IS NOT NULL)),
    CHECK ((identification IS NOT NULL) = (file_edit_revision IS NOT NULL)),
    CHECK ((identification IS 'queued') = (admission IS NOT NULL)),
    CHECK ((identification IN ('running', 'finalizing', 'reading_pick') IS 1)
        = (identify_run IS NOT NULL)),
    CHECK ((identification IS 'reading_pick') = (import_asked IS NOT NULL)),
    CHECK ((identification IN ('not_written', 'inapplicable') IS 1)
        = (save_failure_error IS NOT NULL)),
    CHECK ((import IS NOT NULL) = (import_id IS NOT NULL))
) STRICT;

-- The rows an import holds, which the count of imports in flight reads.
CREATE INDEX IF NOT EXISTS idx_import_candidate_standing_import
    ON import_candidate_standing (import) WHERE import IS NOT NULL;

-- The identifications counted as one batch, for the count the import list
-- draws: each candidate admitted since the batch opened, and whether its
-- identification has ended. Every change to a candidate's identification
-- moves it in that change's own write; the batch is over, and its rows go,
-- once every one has ended. Emptied as the library opens, with the standings.
CREATE TABLE IF NOT EXISTS import_identification_batch (
    candidate_key TEXT PRIMARY KEY,
    ended         INTEGER NOT NULL CHECK (ended IN (0, 1))
) STRICT;

-- Why a candidate's import failed.
CREATE TABLE IF NOT EXISTS import_candidate_failure (
    content_hash      TEXT PRIMARY KEY,
    error             TEXT NOT NULL,
    failed_at         TEXT NOT NULL,
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
    FOREIGN KEY (discogs_library_artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    FOREIGN KEY (musicbrainz_library_artist_id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

-- A person's answer for one conflicting credit in this candidate. The original
-- ids remain evidence; only a merge attaches both to one artist. For
-- 'neither', artist_id reserves the new row's identity; import creates it in
-- the same transaction as the release. These choices are device-local.
CREATE TABLE IF NOT EXISTS import_candidate_artist_identity_choice (
    content_hash TEXT NOT NULL REFERENCES import_candidate_state(content_hash) ON DELETE CASCADE,
    discogs_artist_id TEXT NOT NULL,
    musicbrainz_artist_id TEXT NOT NULL,
    choice TEXT NOT NULL CHECK (choice IN ('same_artist', 'discogs_artist', 'musicbrainz_artist', 'neither')),
    artist_id TEXT NOT NULL,
    PRIMARY KEY (content_hash, discogs_artist_id, musicbrainz_artist_id)
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
    -- A computed state's disc IDs are its `disc_id` signal values, one per
    -- disc, at least one.
    disc_id_state          TEXT NOT NULL CHECK (disc_id_state IN ('computed', 'absent', 'not_cd_audio', 'failed')),
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
    CHECK ((disc_id_state = 'failed') = (disc_id_failure IS NOT NULL)),
    CHECK ((barcode_state = 'failed') = (barcode_failure IS NOT NULL)),
    CHECK ((text_state = 'failed') = (text_failure IS NOT NULL))
) STRICT;

-- The disc IDs, barcodes, catalog numbers and free-text candidates read off a
-- candidate, in reading order.
CREATE TABLE IF NOT EXISTS import_candidate_signal_value (
    content_hash TEXT NOT NULL,
    list         TEXT NOT NULL CHECK (list IN ('disc_id', 'barcode', 'catalog', 'free_text', 'isrc', 'track_title')),
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT NOT NULL,
    -- The candidate-relative path of the file a barcode was read off, or of the
    -- LOG or CUE a disc ID was hashed from, so a surface can mark that file's
    -- row; NULL for a library release's stored files.
    origin_path  TEXT CHECK (origin_path IS NULL OR list IN ('disc_id', 'barcode')),
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
    -- NULL until the source is asked for its tracklist. 'unreadable': it
    -- lists tracks for the audio that cannot be read into values.
    source_tracks_kind  TEXT CHECK (source_tracks_kind IS NULL OR source_tracks_kind IN ('listed', 'nothing', 'unreadable')),
    source_tracks_count INTEGER CHECK (source_tracks_count IS NULL OR source_tracks_count >= 0),
    -- Which of the run's keys returned this release. Matches against the
    -- folder's text are not stored; they are read from the text lines each
    -- time.
    by_disc_id          INTEGER NOT NULL CHECK (by_disc_id IN (0, 1)),
    -- A barcode the folder's files carry.
    by_barcode          INTEGER NOT NULL CHECK (by_barcode IN (0, 1)),
    -- A catalog number in effect: picked, or printed by the folder's text
    -- and carried by a release the run found.
    by_catalog          INTEGER NOT NULL CHECK (by_catalog IN (0, 1)),
    -- The search by the ISRCs the audio's tags carry.
    by_isrc             INTEGER NOT NULL CHECK (by_isrc IN (0, 1)),
    -- The title search, asked only when the folder's own keys and the picked
    -- catalog numbers all came back empty.
    by_search           INTEGER NOT NULL CHECK (by_search IN (0, 1)),
    -- An offered row's pressing, looked up on a catalog the row had no record
    -- of: by the release its page names there, its barcode, or its catalog
    -- number under its label.
    by_pressing         INTEGER NOT NULL CHECK (by_pressing IN (0, 1)),
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
    CHECK (narrowed_out = 0 OR named_note IS NULL),
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

-- ── The import list ───────────────────────────────────────────────────────────

-- Where every row of the import list sits: one row per release the queue
-- lists, and one per header of a group on Found. Its tab, Found's state, the
-- group and dates it sorts by, the text it shows and what its commands are
-- decided from are worked out from a dozen tables, which no index reaches, so
-- the triggers below write them here, indexed, in the same write that changes
-- any of those tables. Computed from device-local rows and never synced.
CREATE TABLE IF NOT EXISTS import_list_placement (
    watched_folder_path TEXT NOT NULL,
    -- 'candidate': a release; 'invalid': a folder that failed validation;
    -- 'group': the header of a group of Found's releases.
    item                TEXT NOT NULL CHECK (item IN ('candidate', 'invalid', 'group')),
    -- A release's key; a group's folder, the first component of its
    -- members' display paths.
    key                 TEXT NOT NULL,
    -- The watched folder's position among the watched folders.
    root_position       INTEGER NOT NULL,
    tab                 TEXT NOT NULL CHECK (tab IN ('pending', 'done', 'skipped')),
    -- Where a Found release stands, what is running for it included.
    pending_state       TEXT CHECK (pending_state IN (
        'not_looked_up', 'identifying', 'needs_you', 'identified', 'unmatched',
        'lookup_error', 'error', 'importing', 'import_error')),
    -- The entry of Found's filter that holds a Found release.
    pending_entry       TEXT GENERATED ALWAYS AS (CASE
        WHEN pending_state IN ('needs_you', 'lookup_error', 'error', 'import_error')
            THEN 'needs_you'
        WHEN pending_state IN ('identifying', 'importing') THEN 'in_progress'
        ELSE pending_state END) VIRTUAL,
    -- Why a release that needs the person does.
    needs_you_reason    TEXT CHECK (needs_you_reason IN (
        'matches', 'no_tracklist', 'medium_mismatch', 'not_found', 'nothing_to_look_up')),
    -- How many pressings its lookup found, for a release that needs the
    -- person because it can't tell them apart or because the folder rules
    -- them all out.
    reason_pressings    INTEGER CHECK (reason_pressings >= 0),
    -- What the folder's own files rule the pressings out as, for the latter.
    reason_medium       TEXT CHECK (reason_medium IN ('cd_rip', 'not_cd_audio')),
    -- How bae broke on its own side, for a release in error.
    error_detail        TEXT,
    -- For a release at a watched folder's top level: whether another release
    -- is listed below its folder, which puts it under that folder's group.
    holds_nested        INTEGER NOT NULL DEFAULT 0 CHECK (holds_nested IN (0, 1)),
    -- The group a Found release is listed under: the first folder of its
    -- display path when it is nested, or its own folder when another release
    -- is listed below it. A header's own.
    group_name          TEXT GENERATED ALWAYS AS (CASE
        WHEN item = 'group' THEN key
        WHEN item = 'candidate' AND tab = 'pending' THEN CASE
            WHEN instr(display_path, '/') > 0
                THEN substr(display_path, 1, instr(display_path, '/') - 1)
            WHEN holds_nested THEN display_path END
        END) STORED,
    -- The release's own date: for Done the time its release was written, in
    -- Unix epoch milliseconds; otherwise its folder's date, as
    -- `folder_discovery` holds it. NULL when none is stored, and for a header.
    own_date            INTEGER,
    -- A group's date, its newest member's own date, on its header and its
    -- members; NULL off a group.
    group_date          INTEGER,
    -- What the row sorts by first: its group's date, else its own.
    unit_date           INTEGER GENERATED ALWAYS AS (CASE
        WHEN group_name IS NULL THEN own_date ELSE group_date END) STORED,
    display_path        TEXT,
    -- What the row sorts by after its date: its group's folder, else its
    -- display path.
    unit_path           TEXT GENERATED ALWAYS AS (COALESCE(group_name, display_path))
        VIRTUAL COLLATE natural_order,
    -- Whether a release shows its draft: once the draft is not blank, or once
    -- a source or a link answered it. Otherwise it shows its folder's name.
    has_summary         INTEGER CHECK (has_summary IN (0, 1)),
    -- The text the row shows: the draft's album title and album artists once
    -- it shows its draft, else the folder's name; for a folder that failed
    -- validation, what its tags call it.
    shown_title         TEXT,
    shown_artists       TEXT,
    -- The shown text case folded as `bae_fold_case` folds it, which is how
    -- the list's filter folds what is typed into it.
    filter_title        TEXT GENERATED ALWAYS AS (bae_fold_case(shown_title)) STORED,
    filter_artists      TEXT GENERATED ALWAYS AS (bae_fold_case(shown_artists)) STORED,
    -- What a release's commands are decided from, beside what is running for
    -- it: whether it can be acted on at all, whether its last import failed,
    -- whether its draft would import, what its stored lookup came to
    -- ('answered', 'failed', or NULL with none stored), and whether a
    -- grouping reads it from several folders. NULL off a release.
    actionable          INTEGER CHECK (actionable IN (0, 1)),
    import_failed       INTEGER CHECK (import_failed IN (0, 1)),
    draft_valid         INTEGER CHECK (draft_valid IN (0, 1)),
    lookup              TEXT CHECK (lookup IN ('answered', 'failed')),
    separable           INTEGER CHECK (separable IN (0, 1)),
    PRIMARY KEY (watched_folder_path, item, key),
    CHECK (item <> 'invalid' OR tab = 'skipped'),
    CHECK ((tab = 'pending' AND item <> 'group') = (pending_state IS NOT NULL)),
    CHECK ((pending_state IS 'needs_you') = (needs_you_reason IS NOT NULL)),
    CHECK (COALESCE(needs_you_reason IN ('matches', 'medium_mismatch'), 0)
           = (reason_pressings IS NOT NULL)),
    CHECK ((needs_you_reason IS 'medium_mismatch') = (reason_medium IS NOT NULL)),
    CHECK ((pending_state IS 'error') = (error_detail IS NOT NULL)),
    CHECK (holds_nested = 0 OR (item <> 'group' AND instr(display_path, '/') = 0)),
    CHECK (item <> 'group' OR tab = 'pending'),
    CHECK ((item = 'group') = (display_path IS NULL)),
    CHECK ((item = 'group') = (shown_title IS NULL)),
    CHECK ((item = 'candidate') = (has_summary IS NOT NULL)),
    CHECK ((item = 'candidate') = (actionable IS NOT NULL)),
    CHECK ((item = 'candidate') = (import_failed IS NOT NULL)),
    CHECK ((item = 'candidate') = (draft_valid IS NOT NULL)),
    CHECK ((item = 'candidate') = (separable IS NOT NULL)),
    CHECK (item = 'candidate' OR lookup IS NULL),
    -- A Done release always has the time its release was written; one stored
    -- in a form this can't read fails the write.
    CHECK (tab <> 'done' OR own_date IS NOT NULL)
) STRICT;

-- A release by its key alone, as a standing or a selection names it.
CREATE INDEX IF NOT EXISTS import_list_placement_by_key ON import_list_placement (key);

-- A group's header and members.
CREATE INDEX IF NOT EXISTS import_list_placement_by_group
    ON import_list_placement (watched_folder_path, group_name);

-- The release at a folder, and those nested below it.
CREATE INDEX IF NOT EXISTS import_list_placement_by_display_path
    ON import_list_placement (watched_folder_path, display_path);

-- Each tab's rows in each order the list shows them in. A group's header
-- leads its members, which share its date and folder; undated rows come last
-- either way; descending path order reverses every term but the header's
-- lead.
CREATE INDEX IF NOT EXISTS import_list_newest_first ON import_list_placement (
    tab, unit_date IS NULL, unit_date DESC, root_position, watched_folder_path, unit_path,
    item = 'group' DESC, own_date IS NULL, own_date DESC,
    display_path COLLATE natural_order, key);

CREATE INDEX IF NOT EXISTS import_list_oldest_first ON import_list_placement (
    tab, unit_date IS NULL, unit_date, root_position, watched_folder_path, unit_path,
    item = 'group' DESC, own_date IS NULL, own_date, display_path COLLATE natural_order, key);

CREATE INDEX IF NOT EXISTS import_list_path_ascending ON import_list_placement (
    tab, root_position, watched_folder_path, unit_path, item = 'group' DESC,
    display_path COLLATE natural_order, key);

CREATE INDEX IF NOT EXISTS import_list_path_descending ON import_list_placement (
    tab, root_position DESC, watched_folder_path DESC, unit_path DESC, item = 'group' DESC,
    display_path COLLATE natural_order DESC, key DESC);

-- Each tab's count, and how many of Found's rows each filter entry holds.
CREATE INDEX IF NOT EXISTS import_list_placement_counts
    ON import_list_placement (tab, item, pending_entry);

-- What the triggers look a changed row's releases up by.
CREATE INDEX IF NOT EXISTS idx_scan_candidate_folder ON scan_candidate (folder);

CREATE INDEX IF NOT EXISTS release_grouping_member_by_folder
    ON release_grouping_member (member_folder);

CREATE INDEX IF NOT EXISTS import_candidate_album_artist_by_artist
    ON import_candidate_album_artist_assignment (artist_id);

-- The releases the queue lists, each as the row it is listed as: settled
-- (one found before the folder around it was known is not listed yet), not
-- taken in by a grouping, and under a watched folder.
CREATE VIEW IF NOT EXISTS import_list_listed AS
SELECT c.watched_folder_path, c.path AS key,
       CASE c.kind WHEN 'invalid' THEN 'invalid' ELSE 'candidate' END AS item,
       w.position AS root_position
FROM scan_candidate AS c
JOIN watched_import_folders AS w ON w.path = c.watched_folder_path
WHERE c.kind <> 'tentative'
  AND NOT EXISTS (SELECT 1 FROM release_grouping_member AS member
                  WHERE member.member_key = c.path);

-- Every release the queue lists, placed: the rules the list's tabs, Found's
-- states, dates, shown text and commands follow, read one release at a time
-- by the triggers that keep `import_list_placement`. Groups are the
-- placement's own (`group_name`).
CREATE VIEW IF NOT EXISTS import_list_entry AS
SELECT watched_folder_path, item, key, root_position, tab, pending_state, needs_you_reason,
       CASE WHEN needs_you_reason IN ('matches', 'medium_mismatch') THEN pressings
            END AS reason_pressings,
       CASE WHEN needs_you_reason = 'medium_mismatch' THEN medium_conflict END AS reason_medium,
       -- A run's answer bae could not store says how it broke; otherwise the
       -- stored verdict does.
       CASE WHEN pending_state = 'error' THEN
                CASE WHEN identification IN ('not_written', 'inapplicable')
                     THEN save_failure_error ELSE verdict_error END
            END AS error_detail,
       own_date, display_path, has_summary, shown_title, shown_artists,
       actionable, import_failed, draft_valid, lookup, separable
FROM (
    SELECT judged.*,
           -- What is running for a Found release decides its state over the
           -- tables: an import, then a run, then a run's answer bae could
           -- not store.
           CASE WHEN tab <> 'pending' THEN NULL
                WHEN running_import IS NOT NULL THEN 'importing'
                WHEN identification IN ('queued', 'running', 'finalizing') THEN 'identifying'
                WHEN identification = 'reading_pick' THEN 'identified'
                WHEN identification IN ('not_written', 'inapplicable') THEN 'error'
                ELSE stored_state END AS pending_state,
           CASE WHEN tab = 'pending' AND running_import IS NULL AND identification IS NULL
                THEN stored_reason END AS needs_you_reason,
           CASE WHEN tab = 'done' THEN imported_at ELSE discovered_at END AS own_date,
           CASE WHEN item = 'invalid' THEN COALESCE(tag_title, name)
                WHEN has_summary THEN draft_title
                ELSE name END AS shown_title,
           CASE WHEN item = 'invalid' THEN tag_artist
                WHEN has_summary THEN COALESCE(credit_line, '') END AS shown_artists,
           CASE WHEN item = 'candidate' THEN tab = 'pending' AND failed END AS import_failed,
           CASE WHEN item = 'candidate' THEN
                -- What the stored lookup came to: failed when a source could
                -- not answer, bae broke, or a found release could not be read
                -- in full.
                CASE WHEN verdict_kind IS NULL THEN NULL
                     WHEN verdict_kind IN ('failed', 'error') THEN 'failed'
                     WHEN declined IS 'unread_document' THEN 'failed'
                     ELSE 'answered' END
           END AS lookup
    FROM (
        SELECT facts.*,
               CASE WHEN kind = 'invalid' THEN 'skipped'
                    WHEN imported THEN 'done'
                    WHEN skipped THEN 'skipped'
                    ELSE 'pending' END AS tab,
               -- Where the tables put a Found release.
               CASE WHEN failed THEN 'import_error'
                    WHEN linked THEN 'identified'
                    WHEN verdict_kind IS NULL THEN 'not_looked_up'
                    WHEN kept_own_draft THEN 'unmatched'
                    WHEN verdict_kind = 'error' THEN 'error'
                    WHEN verdict_kind = 'failed' THEN 'lookup_error'
                    WHEN verdict_kind IN ('not_found', 'manual_only') THEN 'needs_you'
                    WHEN declined IS NULL THEN 'unmatched'
                    WHEN declined = 'unread_document' THEN 'lookup_error'
                    ELSE 'needs_you' END AS stored_state,
               CASE WHEN failed OR linked OR verdict_kind IS NULL OR kept_own_draft THEN NULL
                    WHEN verdict_kind = 'not_found' THEN 'not_found'
                    WHEN verdict_kind = 'manual_only' THEN 'nothing_to_look_up'
                    WHEN verdict_kind <> 'found' THEN NULL
                    WHEN declined = 'nothing_found' THEN 'not_found'
                    WHEN declined = 'several' THEN 'matches'
                    WHEN declined = 'source_tracks_unknown' THEN 'no_tracklist'
                    WHEN declined = 'medium_disagrees' THEN 'medium_mismatch' END AS stored_reason,
               -- The time the release was written, read off its RFC 3339
               -- text: whole seconds through its offset, and the
               -- milliseconds of its fraction, truncated.
               unixepoch(substr(imported_at_text, 1, 19) || imported_at_offset) * 1000
                   + CAST(substr(imported_at_fraction || '000', 1, 3) AS INTEGER) AS imported_at
        FROM (
            SELECT listed.watched_folder_path, listed.key, listed.item, listed.root_position,
                   c.kind, c.display_path, c.name,
                   -- A scanned release is dated by its folder; a grouping's
                   -- by the earliest of the folders it takes in.
                   CASE c.source_kind
                       WHEN 'folder' THEN
                           (SELECT COALESCE(d.source_date, d.first_seen_at)
                            FROM folder_discovery AS d WHERE d.folder = c.folder)
                       WHEN 'grouping' THEN
                           (SELECT MIN(COALESCE(d.source_date, d.first_seen_at))
                            FROM release_grouping_member AS m
                            JOIN folder_discovery AS d ON d.folder = m.member_folder
                            WHERE m.grouping_key = c.grouping_key)
                   END AS discovered_at,
                   -- Only what this device imported places a folder in Done.
                   origin.release_id IS NOT NULL AS imported,
                   released.created_at AS imported_at_text,
                   CASE WHEN substr(released.created_at, 20, 1) = '.'
                        THEN ltrim(substr(released.created_at, 21), '0123456789')
                        ELSE substr(released.created_at, 20) END AS imported_at_offset,
                   CASE WHEN substr(released.created_at, 20, 1) = '.'
                        THEN substr(released.created_at, 21,
                                    length(released.created_at) - 20
                                        - length(ltrim(substr(released.created_at, 21),
                                                       '0123456789')))
                        ELSE '' END AS imported_at_fraction,
                   CASE WHEN c.grouping_key IS NOT NULL THEN g.skipped
                        ELSE EXISTS (SELECT 1 FROM skipped_import_candidates AS k
                                     WHERE k.candidate_path = c.folder) END AS skipped,
                   g.blocked IS NOT NULL
                       OR EXISTS (SELECT 1 FROM import_candidate_failure AS f
                                  WHERE f.content_hash = c.content_hash) AS failed,
                   CASE WHEN listed.item = 'candidate' THEN g.blocked IS NULL END AS actionable,
                   CASE WHEN listed.item = 'candidate' THEN c.grouping_key IS NOT NULL
                        END AS separable,
                   CASE WHEN listed.item = 'candidate' THEN COALESCE(e.draft_valid, 0)
                        END AS draft_valid,
                   -- The draft, the link and the verdict count only while the
                   -- state describes the folder's current files.
                   EXISTS (SELECT 1 FROM import_candidate_release_link AS l
                           WHERE l.content_hash = s.content_hash) AS linked,
                   v.kind AS verdict_kind,
                   v.kept_own_draft,
                   v.medium_conflict,
                   v.error_detail AS verdict_error,
                   -- How many pressings the releases agreement kept make.
                   (SELECT COUNT(DISTINCT m.pressing) FROM import_candidate_match AS m
                    WHERE m.content_hash = v.content_hash AND m.narrowed_out = 0) AS pressings,
                   -- Why a found verdict picks none of its releases
                   -- unattended, judged from what it found that agreement
                   -- kept, led by the lowest position.
                   CASE WHEN v.kind <> 'found' THEN NULL
                        WHEN v.medium_conflict IS NOT NULL THEN 'medium_disagrees'
                        ELSE (SELECT CASE
                                  WHEN lead.position IS NULL THEN 'nothing_found'
                                  WHEN (SELECT COUNT(DISTINCT m.pressing)
                                        FROM import_candidate_match AS m
                                        WHERE m.content_hash = v.content_hash
                                          AND m.narrowed_out = 0) <> 1
                                      THEN 'several'
                                  WHEN EXISTS (SELECT 1 FROM import_candidate_match AS m
                                               WHERE m.content_hash = v.content_hash
                                                 AND m.narrowed_out = 0
                                                 AND m.document_failure IS NOT NULL)
                                      THEN 'unread_document'
                                  WHEN lead.source_tracks_kind = 'listed'
                                   AND lead.source_tracks_count = COALESCE(v.track_count, 0)
                                      THEN NULL
                                  WHEN lead.source_tracks_kind IN ('listed', 'unreadable')
                                      THEN 'nothing_found'
                                  ELSE 'source_tracks_unknown' END
                              FROM (SELECT NULL) AS one
                              LEFT JOIN import_candidate_match AS lead
                                  ON lead.content_hash = v.content_hash
                                 AND lead.position = (SELECT MIN(m.position)
                                                      FROM import_candidate_match AS m
                                                      WHERE m.content_hash = v.content_hash
                                                        AND m.narrowed_out = 0))
                   END AS declined,
                   st.identification, st.import AS running_import, st.save_failure_error,
                   t.album_title AS tag_title, t.artist AS tag_artist,
                   e.album_title AS draft_title,
                   CASE WHEN listed.item = 'candidate' THEN
                       e.content_hash IS NOT NULL
                       AND (NOT e.draft_blank
                            OR EXISTS (SELECT 1 FROM import_candidate_draft_provenance AS p
                                       WHERE p.content_hash = e.content_hash)
                            OR EXISTS (SELECT 1 FROM import_candidate_release_link AS l
                                       WHERE l.content_hash = e.content_hash))
                   END AS has_summary,
                   -- The draft's album credits as one line: each printed
                   -- name, after the join the credit before it states, or a
                   -- comma.
                   (SELECT group_concat(
                           COALESCE((SELECT COALESCE(before.join_phrase, ', ')
                                     FROM import_candidate_album_artist_assignment AS before
                                     WHERE before.content_hash = a.content_hash
                                       AND before.position < a.position
                                     ORDER BY before.position DESC LIMIT 1), '')
                           || CASE a.assignment_kind
                                  WHEN 'picked' THEN COALESCE(credit.name, artist.name)
                                  ELSE COALESCE(a.credited_as, a.name) END,
                           '' ORDER BY a.position)
                    FROM import_candidate_album_artist_assignment AS a
                    LEFT JOIN artists AS artist ON artist.id = a.artist_id
                    LEFT JOIN artist_credits AS credit
                        ON credit.artist_id = a.artist_id AND credit.name_key = a.credit_key
                    WHERE a.content_hash = e.content_hash) AS credit_line
            FROM import_list_listed AS listed
            JOIN scan_candidate AS c
                ON c.watched_folder_path = listed.watched_folder_path AND c.path = listed.key
            LEFT JOIN release_grouping AS g ON g.key = c.grouping_key
            LEFT JOIN import_candidate_state AS s
                ON s.content_hash = c.content_hash AND s.edit_revision = c.file_edit_revision
            LEFT JOIN import_candidate_edit AS e ON e.content_hash = s.content_hash
            LEFT JOIN import_candidate_verdict AS v ON v.content_hash = s.content_hash
            LEFT JOIN local_release_origin AS origin ON origin.content_hash = c.content_hash
            LEFT JOIN releases AS released ON released.id = origin.release_id
            LEFT JOIN import_candidate_standing AS st ON st.candidate_key = c.path
            LEFT JOIN scan_skipped_tags AS t
                ON t.watched_folder_path = c.watched_folder_path AND t.candidate_path = c.path
        ) AS facts
    ) AS judged
) AS placed;

-- Place the release at `key` again from its tables: SQLite has no procedures,
-- so the triggers that keep `import_list_placement` name a release to place
-- by inserting its key here, and the trigger on this view does the placing.
-- Nothing is ever stored in it.
CREATE VIEW IF NOT EXISTS import_list_replace AS
SELECT path AS key FROM scan_candidate WHERE 0;

CREATE TRIGGER IF NOT EXISTS import_list_replace_release INSTEAD OF INSERT ON import_list_replace
BEGIN
    INSERT INTO import_list_placement
        (watched_folder_path, item, key, root_position, tab, pending_state, needs_you_reason,
         reason_pressings, reason_medium, error_detail, own_date, display_path, has_summary,
         shown_title, shown_artists, actionable, import_failed, draft_valid, lookup, separable)
    SELECT watched_folder_path, item, key, root_position, tab, pending_state, needs_you_reason,
           reason_pressings, reason_medium, error_detail, own_date, display_path, has_summary,
           shown_title, shown_artists, actionable, import_failed, draft_valid, lookup, separable
    FROM import_list_entry WHERE key = NEW.key
    ON CONFLICT (watched_folder_path, item, key) DO UPDATE SET
        root_position = excluded.root_position,
        tab = excluded.tab,
        pending_state = excluded.pending_state,
        needs_you_reason = excluded.needs_you_reason,
        reason_pressings = excluded.reason_pressings,
        reason_medium = excluded.reason_medium,
        error_detail = excluded.error_detail,
        own_date = excluded.own_date,
        display_path = excluded.display_path,
        has_summary = excluded.has_summary,
        shown_title = excluded.shown_title,
        shown_artists = excluded.shown_artists,
        actionable = excluded.actionable,
        import_failed = excluded.import_failed,
        draft_valid = excluded.draft_valid,
        lookup = excluded.lookup,
        separable = excluded.separable
    WHERE (import_list_placement.root_position, import_list_placement.tab,
           import_list_placement.pending_state, import_list_placement.needs_you_reason,
           import_list_placement.reason_pressings, import_list_placement.reason_medium,
           import_list_placement.error_detail, import_list_placement.own_date,
           import_list_placement.display_path, import_list_placement.has_summary,
           import_list_placement.shown_title, import_list_placement.shown_artists,
           import_list_placement.actionable, import_list_placement.import_failed,
           import_list_placement.draft_valid, import_list_placement.lookup,
           import_list_placement.separable)
        IS NOT (excluded.root_position, excluded.tab, excluded.pending_state,
                excluded.needs_you_reason, excluded.reason_pressings, excluded.reason_medium,
                excluded.error_detail, excluded.own_date, excluded.display_path,
                excluded.has_summary, excluded.shown_title, excluded.shown_artists,
                excluded.actionable, excluded.import_failed, excluded.draft_valid,
                excluded.lookup, excluded.separable);
    -- A release that is no longer this item, or no longer listed, goes;
    -- after its new row is in, so what it was selected as stays selected.
    DELETE FROM import_list_placement
    WHERE key = NEW.key AND item <> 'group'
      AND NOT EXISTS (SELECT 1 FROM import_list_listed AS listed
                      WHERE listed.key = NEW.key
                        AND listed.watched_folder_path = import_list_placement.watched_folder_path
                        AND listed.item = import_list_placement.item);
END;

-- Bring the header of the group `group_name` under `watched_folder_path` in
-- line with its members: there while it has any, and dated with them by the
-- newest member's own date. Like `import_list_replace`, a procedure entry
-- point that stores nothing.
CREATE VIEW IF NOT EXISTS import_list_regroup AS
SELECT watched_folder_path, group_name FROM import_list_placement WHERE 0;

CREATE TRIGGER IF NOT EXISTS import_list_regroup_header INSTEAD OF INSERT ON import_list_regroup
BEGIN
    INSERT INTO import_list_placement
        (watched_folder_path, item, key, root_position, tab)
    SELECT NEW.watched_folder_path, 'group', NEW.group_name, w.position, 'pending'
    FROM watched_import_folders AS w
    WHERE w.path = NEW.watched_folder_path
      AND EXISTS (SELECT 1 FROM import_list_placement AS member
                  WHERE member.watched_folder_path = NEW.watched_folder_path
                    AND member.group_name = NEW.group_name AND member.item <> 'group')
    ON CONFLICT (watched_folder_path, item, key) DO NOTHING;
    DELETE FROM import_list_placement
    WHERE watched_folder_path = NEW.watched_folder_path AND item = 'group' AND key = NEW.group_name
      AND NOT EXISTS (SELECT 1 FROM import_list_placement AS member
                      WHERE member.watched_folder_path = NEW.watched_folder_path
                        AND member.group_name = NEW.group_name AND member.item <> 'group');
    UPDATE import_list_placement
    SET group_date = (SELECT MAX(member.own_date) FROM import_list_placement AS member
                      WHERE member.watched_folder_path = NEW.watched_folder_path
                        AND member.group_name = NEW.group_name AND member.item <> 'group')
    WHERE watched_folder_path = NEW.watched_folder_path AND group_name = NEW.group_name
      AND group_date IS NOT (SELECT MAX(member.own_date) FROM import_list_placement AS member
                             WHERE member.watched_folder_path = NEW.watched_folder_path
                               AND member.group_name = NEW.group_name
                               AND member.item <> 'group');
END;


-- Whether another release is listed below the folder of each row at a
-- watched folder's top level: what puts that row under its folder's group.
CREATE VIEW IF NOT EXISTS import_list_nesting AS
SELECT p.watched_folder_path, p.item, p.key, p.display_path,
       EXISTS (SELECT 1 FROM import_list_placement AS nested
               WHERE nested.watched_folder_path = p.watched_folder_path
                 AND nested.display_path > p.display_path || '/'
                 AND nested.display_path < p.display_path || '0') AS holds_nested
FROM import_list_placement AS p
WHERE p.item <> 'group' AND instr(p.display_path, '/') = 0;

-- Bring whether the top-level row at `display_path` under
-- `watched_folder_path` holds a nested release in line with the rows below
-- it. A procedure entry point that stores nothing, like `import_list_replace`.
CREATE VIEW IF NOT EXISTS import_list_renest AS
SELECT watched_folder_path, display_path FROM import_list_placement WHERE 0;

CREATE TRIGGER IF NOT EXISTS import_list_renest_row INSTEAD OF INSERT ON import_list_renest
BEGIN
    UPDATE import_list_placement
    SET holds_nested = nesting.holds_nested,
        group_date = CASE WHEN nesting.holds_nested THEN import_list_placement.group_date END
    FROM import_list_nesting AS nesting
    WHERE nesting.watched_folder_path = NEW.watched_folder_path
      AND nesting.display_path = NEW.display_path
      AND import_list_placement.watched_folder_path = nesting.watched_folder_path
      AND import_list_placement.item = nesting.item
      AND import_list_placement.key = nesting.key
      AND import_list_placement.holds_nested IS NOT nesting.holds_nested;
END;

-- A row placed, moved or gone changes the group it is listed under, and
-- whether the top-level row at the folder it is in holds a nested release:
-- itself, when it is that row.
CREATE TRIGGER IF NOT EXISTS import_list_placement_added AFTER INSERT ON import_list_placement
WHEN NEW.item <> 'group'
BEGIN
    INSERT INTO import_list_renest (watched_folder_path, display_path)
    VALUES (NEW.watched_folder_path,
            CASE WHEN instr(NEW.display_path, '/') > 0
                 THEN substr(NEW.display_path, 1, instr(NEW.display_path, '/') - 1)
                 ELSE NEW.display_path END);
    INSERT INTO import_list_regroup (watched_folder_path, group_name)
    SELECT NEW.watched_folder_path, NEW.group_name WHERE NEW.group_name IS NOT NULL;
END;

CREATE TRIGGER IF NOT EXISTS import_list_placement_removed AFTER DELETE ON import_list_placement
WHEN OLD.item <> 'group'
BEGIN
    INSERT INTO import_list_renest (watched_folder_path, display_path)
    SELECT OLD.watched_folder_path,
           substr(OLD.display_path, 1, instr(OLD.display_path, '/') - 1)
    WHERE instr(OLD.display_path, '/') > 0;
    INSERT INTO import_list_regroup (watched_folder_path, group_name)
    SELECT OLD.watched_folder_path, OLD.group_name WHERE OLD.group_name IS NOT NULL;
END;

CREATE TRIGGER IF NOT EXISTS import_list_placement_moved AFTER UPDATE ON import_list_placement
WHEN NEW.item <> 'group' AND OLD.display_path IS NOT NEW.display_path
BEGIN
    INSERT INTO import_list_renest (watched_folder_path, display_path)
    SELECT OLD.watched_folder_path,
           substr(OLD.display_path, 1, instr(OLD.display_path, '/') - 1)
    WHERE instr(OLD.display_path, '/') > 0
    UNION
    SELECT NEW.watched_folder_path,
           CASE WHEN instr(NEW.display_path, '/') > 0
                THEN substr(NEW.display_path, 1, instr(NEW.display_path, '/') - 1)
                ELSE NEW.display_path END;
END;

CREATE TRIGGER IF NOT EXISTS import_list_placement_regrouped AFTER UPDATE ON import_list_placement
WHEN NEW.item <> 'group'
 AND (OLD.group_name IS NOT NEW.group_name OR OLD.own_date IS NOT NEW.own_date)
BEGIN
    -- A row out of every group carries no group's date.
    UPDATE import_list_placement SET group_date = NULL
    WHERE NEW.group_name IS NULL AND NEW.group_date IS NOT NULL
      AND watched_folder_path = NEW.watched_folder_path AND item = NEW.item AND key = NEW.key;
    INSERT INTO import_list_regroup (watched_folder_path, group_name)
    SELECT OLD.watched_folder_path, OLD.group_name WHERE OLD.group_name IS NOT NULL
    UNION
    SELECT NEW.watched_folder_path, NEW.group_name WHERE NEW.group_name IS NOT NULL;
END;

-- Each import list a surface has open, as it is viewed: its tab, filter
-- entry and filter text, and its order; which of its groups are folded shut
-- (`import_list_collapsed`); and the rows selected in it
-- (`import_list_selection`). What a list shows is a query over these and the
-- placement, so a change to any of them is a write the list's live query
-- answers. The revisions count changes to the view, its folds included, and
-- to the selection, so a surface can wait for the list value that reflects
-- one. No list outlives the process that opened it: the import service
-- empties these tables as the library opens.
CREATE TABLE IF NOT EXISTS import_list (
    id                 INTEGER PRIMARY KEY,
    tab                TEXT NOT NULL CHECK (tab IN ('pending', 'done', 'skipped')),
    pending_filter     TEXT NOT NULL CHECK (pending_filter IN (
        'all', 'needs_you', 'in_progress', 'identified', 'unmatched', 'not_looked_up')),
    filter_text        TEXT NOT NULL,
    -- What a row's shown text must contain, case folded as `filter_title`
    -- is; empty keeps every row.
    needle             TEXT GENERATED ALWAYS AS (bae_fold_case(filter_text)) STORED,
    list_order         TEXT NOT NULL CHECK (list_order IN (
        'newest_first', 'oldest_first', 'path_ascending', 'path_descending')),
    view_revision      INTEGER NOT NULL DEFAULT 0 CHECK (view_revision >= 0),
    selection_revision INTEGER NOT NULL DEFAULT 0 CHECK (selection_revision >= 0)
) STRICT;

-- The groups a list shows folded shut: their members are not in it. A group
-- that is gone takes its folded state with it.
CREATE TABLE IF NOT EXISTS import_list_collapsed (
    list_id             INTEGER NOT NULL REFERENCES import_list (id) ON DELETE CASCADE,
    watched_folder_path TEXT NOT NULL,
    item                TEXT NOT NULL DEFAULT 'group' CHECK (item = 'group'),
    group_name          TEXT NOT NULL,
    PRIMARY KEY (list_id, watched_folder_path, group_name),
    FOREIGN KEY (watched_folder_path, item, group_name)
        REFERENCES import_list_placement (watched_folder_path, item, key) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS import_list_collapsed_by_group
    ON import_list_collapsed (watched_folder_path, item, group_name);

-- The rows selected in a list, by key: only ever rows it shows. A row the
-- list stops showing, by a change to its view or to the row, leaves the
-- selection in that write.
CREATE TABLE IF NOT EXISTS import_list_selection (
    list_id INTEGER NOT NULL REFERENCES import_list (id) ON DELETE CASCADE,
    key     TEXT NOT NULL,
    PRIMARY KEY (list_id, key)
) STRICT;

CREATE INDEX IF NOT EXISTS import_list_selection_by_key ON import_list_selection (key);

-- Each list's view as the queries below read it: whether its filters
-- narrow its tab, by filter text or by a filter entry on Found, and whether
-- it keeps every row of its tab, its filters narrowing nothing and no group
-- folded shut.
CREATE VIEW IF NOT EXISTS import_list_view AS
SELECT l.*,
       NOT l.narrows
       AND NOT EXISTS (SELECT 1 FROM import_list_collapsed AS c WHERE c.list_id = l.id)
           AS keeps_all
FROM (SELECT *, needle <> '' OR (tab = 'pending' AND pending_filter <> 'all') AS narrows
      FROM import_list) AS l;

-- Each list's rows of its tab, each with whether its filters keep it: on
-- Found, in its filter entry; and with its shown text containing the
-- needle. The tab is matched by value rather than by index (`+p.tab`), so a
-- lookup of a group's members reaches them by their group.
CREATE VIEW IF NOT EXISTS import_list_judged AS
SELECT v.id AS list_id, v.keeps_all, p.*,
       (v.tab <> 'pending' OR v.pending_filter = 'all' OR p.pending_entry = v.pending_filter)
       AND (v.needle = '' OR instr(p.filter_title, v.needle) > 0
            OR instr(p.filter_artists, v.needle) > 0) AS kept
FROM import_list_view AS v
JOIN import_list_placement AS p ON +p.tab = v.tab;

-- Each list's releases its filters keep, folded groups' members included.
CREATE VIEW IF NOT EXISTS import_list_matching AS
SELECT * FROM import_list_judged WHERE item <> 'group' AND kept;

-- Each list's items, in no order: the releases its filters keep outside the
-- groups it folds shut, and the header of each group holding a release its
-- filters keep.
CREATE VIEW IF NOT EXISTS import_list_item AS
SELECT * FROM import_list_judged AS j
WHERE j.keeps_all
   OR CASE WHEN j.item = 'group'
           THEN EXISTS (SELECT 1 FROM import_list_matching AS m
                        WHERE m.list_id = j.list_id
                          AND m.watched_folder_path = j.watched_folder_path
                          AND m.group_name = j.key)
           ELSE j.kept
                AND (j.group_name IS NULL
                     OR NOT EXISTS (SELECT 1 FROM import_list_collapsed AS c
                                    WHERE c.list_id = j.list_id
                                      AND c.watched_folder_path = j.watched_folder_path
                                      AND c.group_name = j.group_name)) END;

-- Each list's releases it shows.
CREATE VIEW IF NOT EXISTS import_list_shown AS
SELECT * FROM import_list_item WHERE item <> 'group';

-- A selected release a change moves out of what a list shows leaves its
-- selection; its selection's revision moves with it.
CREATE TRIGGER IF NOT EXISTS import_list_selected_moved AFTER UPDATE ON import_list_placement
WHEN NEW.item <> 'group'
 AND (OLD.tab, OLD.pending_state, OLD.filter_title, OLD.filter_artists, OLD.group_name)
     IS NOT (NEW.tab, NEW.pending_state, NEW.filter_title, NEW.filter_artists, NEW.group_name)
BEGIN
    DELETE FROM import_list_selection
    WHERE key = NEW.key
      AND NOT EXISTS (SELECT 1 FROM import_list_shown AS shown
                      WHERE shown.list_id = import_list_selection.list_id
                        AND shown.key = NEW.key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_selected_removed AFTER DELETE ON import_list_placement
WHEN OLD.item <> 'group'
BEGIN
    DELETE FROM import_list_selection
    WHERE key = OLD.key
      AND NOT EXISTS (SELECT 1 FROM import_list_shown AS shown
                      WHERE shown.list_id = import_list_selection.list_id
                        AND shown.key = OLD.key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_view_changed
AFTER UPDATE OF tab, pending_filter, filter_text ON import_list
WHEN (OLD.tab, OLD.pending_filter, OLD.filter_text)
     IS NOT (NEW.tab, NEW.pending_filter, NEW.filter_text)
BEGIN
    DELETE FROM import_list_selection
    WHERE list_id = NEW.id
      AND NOT EXISTS (SELECT 1 FROM import_list_shown AS shown
                      WHERE shown.list_id = NEW.id AND shown.key = import_list_selection.key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_group_folded AFTER INSERT ON import_list_collapsed
BEGIN
    DELETE FROM import_list_selection
    WHERE list_id = NEW.list_id
      AND key IN (SELECT key FROM import_list_placement
                  WHERE watched_folder_path = NEW.watched_folder_path
                    AND group_name = NEW.group_name AND item <> 'group');
END;

-- Every change to a selection, the person's or one that hid a row, moves its
-- revision.
CREATE TRIGGER IF NOT EXISTS import_list_selection_added AFTER INSERT ON import_list_selection
BEGIN
    UPDATE import_list SET selection_revision = selection_revision + 1 WHERE id = NEW.list_id;
END;

CREATE TRIGGER IF NOT EXISTS import_list_selection_removed AFTER DELETE ON import_list_selection
BEGIN
    UPDATE import_list SET selection_revision = selection_revision + 1 WHERE id = OLD.list_id;
END;

-- Every table the rules read places again the releases a change to it can
-- move. A change to a column the rules don't read places nothing.

CREATE TRIGGER IF NOT EXISTS import_list_on_candidate_added AFTER INSERT ON scan_candidate
BEGIN
    INSERT INTO import_list_replace (key) VALUES (NEW.path);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_candidate_removed AFTER DELETE ON scan_candidate
BEGIN
    INSERT INTO import_list_replace (key) VALUES (OLD.path);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_candidate_changed AFTER UPDATE OF
    watched_folder_path, path, kind, name, display_path, folder, content_hash,
    file_edit_revision, grouping_key, source_kind ON scan_candidate
WHEN (OLD.watched_folder_path, OLD.path, OLD.kind, OLD.name, OLD.display_path, OLD.folder,
      OLD.content_hash, OLD.file_edit_revision, OLD.grouping_key, OLD.source_kind)
  IS NOT (NEW.watched_folder_path, NEW.path, NEW.kind, NEW.name, NEW.display_path, NEW.folder,
          NEW.content_hash, NEW.file_edit_revision, NEW.grouping_key, NEW.source_kind)
BEGIN
    INSERT INTO import_list_replace (key) SELECT OLD.path UNION SELECT NEW.path;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_watched_folder_moved
AFTER UPDATE OF position ON watched_import_folders
WHEN OLD.position IS NOT NEW.position
BEGIN
    UPDATE import_list_placement SET root_position = NEW.position
    WHERE watched_folder_path = NEW.path;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_discovery_added AFTER INSERT ON folder_discovery
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder = NEW.folder
    UNION SELECT grouping_key FROM release_grouping_member WHERE member_folder = NEW.folder;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_discovery_removed AFTER DELETE ON folder_discovery
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder = OLD.folder
    UNION SELECT grouping_key FROM release_grouping_member WHERE member_folder = OLD.folder;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_discovery_changed
AFTER UPDATE OF folder, first_seen_at, source_date ON folder_discovery
WHEN (OLD.folder, OLD.first_seen_at, OLD.source_date)
  IS NOT (NEW.folder, NEW.first_seen_at, NEW.source_date)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder IN (OLD.folder, NEW.folder)
    UNION SELECT grouping_key FROM release_grouping_member
          WHERE member_folder IN (OLD.folder, NEW.folder);
END;

-- Removing a grouping removes its release, by foreign key, which places it
-- out of the list.
CREATE TRIGGER IF NOT EXISTS import_list_on_grouping_added AFTER INSERT ON release_grouping
BEGIN
    INSERT INTO import_list_replace (key) VALUES (NEW.key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_grouping_changed
AFTER UPDATE OF key, skipped, blocked ON release_grouping
WHEN (OLD.key, OLD.skipped, OLD.blocked) IS NOT (NEW.key, NEW.skipped, NEW.blocked)
BEGIN
    INSERT INTO import_list_replace (key) SELECT OLD.key UNION SELECT NEW.key;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_grouping_member_added
AFTER INSERT ON release_grouping_member
BEGIN
    INSERT INTO import_list_replace (key) VALUES (NEW.member_key), (NEW.grouping_key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_grouping_member_removed
AFTER DELETE ON release_grouping_member
BEGIN
    INSERT INTO import_list_replace (key) VALUES (OLD.member_key), (OLD.grouping_key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_grouping_member_changed
AFTER UPDATE ON release_grouping_member
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT OLD.member_key UNION SELECT OLD.grouping_key
    UNION SELECT NEW.member_key UNION SELECT NEW.grouping_key;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skip_added AFTER INSERT ON skipped_import_candidates
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder = NEW.candidate_path;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skip_removed AFTER DELETE ON skipped_import_candidates
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder = OLD.candidate_path;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skip_changed AFTER UPDATE ON skipped_import_candidates
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder IN (OLD.candidate_path, NEW.candidate_path);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skipped_tags_added AFTER INSERT ON scan_skipped_tags
BEGIN
    INSERT INTO import_list_replace (key) VALUES (NEW.candidate_path);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skipped_tags_removed AFTER DELETE ON scan_skipped_tags
BEGIN
    INSERT INTO import_list_replace (key) VALUES (OLD.candidate_path);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skipped_tags_changed AFTER UPDATE ON scan_skipped_tags
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT OLD.candidate_path UNION SELECT NEW.candidate_path;
END;

-- Storing or removing a state places nothing by itself: everything the rules
-- read of it hangs off it by foreign key, written after it and deleted with
-- it, and those rows' own triggers place the release. Moving the revision it
-- describes does.
CREATE TRIGGER IF NOT EXISTS import_list_on_state_changed
AFTER UPDATE OF content_hash, edit_revision ON import_candidate_state
WHEN (OLD.content_hash, OLD.edit_revision) IS NOT (NEW.content_hash, NEW.edit_revision)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_draft_added AFTER INSERT ON import_candidate_edit
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_draft_removed AFTER DELETE ON import_candidate_edit
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_draft_changed
AFTER UPDATE OF content_hash, album_title, draft_blank, draft_valid ON import_candidate_edit
WHEN (OLD.content_hash, OLD.album_title, OLD.draft_blank, OLD.draft_valid)
  IS NOT (NEW.content_hash, NEW.album_title, NEW.draft_blank, NEW.draft_valid)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_album_artist_added AFTER INSERT ON import_candidate_album_artist_assignment
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_album_artist_removed AFTER DELETE ON import_candidate_album_artist_assignment
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_album_artist_changed
AFTER UPDATE OF content_hash, position, assignment_kind, artist_id, name, credit_key, credited_as, join_phrase ON import_candidate_album_artist_assignment
WHEN (OLD.content_hash, OLD.position, OLD.assignment_kind, OLD.artist_id, OLD.name, OLD.credit_key, OLD.credited_as, OLD.join_phrase) IS NOT (NEW.content_hash, NEW.position, NEW.assignment_kind, NEW.artist_id, NEW.name, NEW.credit_key, NEW.credited_as, NEW.join_phrase)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_provenance_added AFTER INSERT ON import_candidate_draft_provenance
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_provenance_removed AFTER DELETE ON import_candidate_draft_provenance
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_provenance_changed
AFTER UPDATE OF content_hash ON import_candidate_draft_provenance
WHEN OLD.content_hash IS NOT NEW.content_hash
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_link_added AFTER INSERT ON import_candidate_release_link
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_link_removed AFTER DELETE ON import_candidate_release_link
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_link_changed
AFTER UPDATE OF content_hash ON import_candidate_release_link
WHEN OLD.content_hash IS NOT NEW.content_hash
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_verdict_added AFTER INSERT ON import_candidate_verdict
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_verdict_removed AFTER DELETE ON import_candidate_verdict
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_verdict_changed
AFTER UPDATE OF content_hash, kind, track_count, medium_conflict, error_detail, kept_own_draft
ON import_candidate_verdict
WHEN (OLD.content_hash, OLD.kind, OLD.track_count, OLD.medium_conflict, OLD.error_detail,
      OLD.kept_own_draft)
  IS NOT (NEW.content_hash, NEW.kind, NEW.track_count, NEW.medium_conflict, NEW.error_detail,
          NEW.kept_own_draft)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_match_added AFTER INSERT ON import_candidate_match
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_match_removed AFTER DELETE ON import_candidate_match
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_match_changed
AFTER UPDATE OF content_hash, position, pressing, narrowed_out, source_tracks_kind, source_tracks_count, document_failure ON import_candidate_match
WHEN (OLD.content_hash, OLD.position, OLD.pressing, OLD.narrowed_out, OLD.source_tracks_kind, OLD.source_tracks_count, OLD.document_failure) IS NOT (NEW.content_hash, NEW.position, NEW.pressing, NEW.narrowed_out, NEW.source_tracks_kind, NEW.source_tracks_count, NEW.document_failure)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_failure_added AFTER INSERT ON import_candidate_failure
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_failure_removed AFTER DELETE ON import_candidate_failure
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_failure_changed
AFTER UPDATE OF content_hash ON import_candidate_failure
WHEN OLD.content_hash IS NOT NEW.content_hash
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_origin_added AFTER INSERT ON local_release_origin
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_origin_removed AFTER DELETE ON local_release_origin
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_origin_changed
AFTER UPDATE OF content_hash, release_id ON local_release_origin
WHEN (OLD.content_hash, OLD.release_id) IS NOT (NEW.content_hash, NEW.release_id)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_standing_added AFTER INSERT ON import_candidate_standing
BEGIN
    INSERT INTO import_list_replace (key) VALUES (NEW.candidate_key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_standing_removed AFTER DELETE ON import_candidate_standing
BEGIN
    INSERT INTO import_list_replace (key) VALUES (OLD.candidate_key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_standing_changed
AFTER UPDATE OF candidate_key, identification, save_failure_error, import
ON import_candidate_standing
WHEN (OLD.candidate_key, OLD.identification, OLD.save_failure_error, OLD.import)
  IS NOT (NEW.candidate_key, NEW.identification, NEW.save_failure_error, NEW.import)
BEGIN
    INSERT INTO import_list_replace (key) SELECT OLD.candidate_key UNION SELECT NEW.candidate_key;
END;

-- A picked artist's name, or the credit name the draft prints for it.
CREATE TRIGGER IF NOT EXISTS import_list_on_artist_renamed AFTER UPDATE OF name ON artists
WHEN OLD.name IS NOT NEW.name
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM import_candidate_album_artist_assignment AS a
    JOIN scan_candidate AS c ON c.content_hash = a.content_hash
    WHERE a.artist_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_artist_credit_added AFTER INSERT ON artist_credits
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM import_candidate_album_artist_assignment AS a
    JOIN scan_candidate AS c ON c.content_hash = a.content_hash
    WHERE a.artist_id = NEW.artist_id AND a.credit_key = NEW.name_key;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_artist_credit_removed AFTER DELETE ON artist_credits
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM import_candidate_album_artist_assignment AS a
    JOIN scan_candidate AS c ON c.content_hash = a.content_hash
    WHERE a.artist_id = OLD.artist_id AND a.credit_key = OLD.name_key;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_artist_credit_changed
AFTER UPDATE OF artist_id, name_key, name ON artist_credits
WHEN (OLD.artist_id, OLD.name_key, OLD.name) IS NOT (NEW.artist_id, NEW.name_key, NEW.name)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM import_candidate_album_artist_assignment AS a
    JOIN scan_candidate AS c ON c.content_hash = a.content_hash
    WHERE (a.artist_id = OLD.artist_id AND a.credit_key = OLD.name_key)
       OR (a.artist_id = NEW.artist_id AND a.credit_key = NEW.name_key);
END;

-- The time an imported release was written orders it on Done.
CREATE TRIGGER IF NOT EXISTS import_list_on_release_dated AFTER UPDATE OF created_at ON releases
WHEN OLD.created_at IS NOT NEW.created_at
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM local_release_origin AS o
    JOIN scan_candidate AS c ON c.content_hash = o.content_hash
    WHERE o.release_id = NEW.id;
END;

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
    -- Every barcode the record states, in its order: a JSON array of strings.
    barcodes           TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(barcodes) AND json_type(barcodes) = 'array'),
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
    -- What a MusicBrainz release's documents state its album is on the other
    -- lookup catalog: 'read', the albums named in source_release_album_link
    -- (none, where nothing names one), or 'unread', where a document a
    -- statement reads was not fetched and none named an album. NULL for a
    -- Discogs release, whose documents state no counterpart album.
    album_links        TEXT CHECK (album_links IS NULL OR album_links IN ('read', 'unread')),
    -- The MusicBrainz release whose Cover Art Archive gallery the picker opens
    -- (this release, or the one a Discogs release cross-references), and its
    -- release group.
    archive_release_id TEXT,
    archive_group_id   TEXT,
    fetched_at         TEXT NOT NULL,
    PRIMARY KEY (catalog, release_id),
    CHECK (archive_release_id IS NOT NULL OR archive_group_id IS NULL),
    CHECK (country IS NULL OR region IS NULL),
    CHECK ((catalog = 'musicbrainz') = (album_links IS NOT NULL))
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
    -- The name the catalog prints for the credit, where it is not `name`.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    -- What the catalog prints after the credit when another follows.
    join_phrase           TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
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

-- Each album on another catalog a MusicBrainz release's documents name as its
-- album, for a release whose album_links is 'read', in statement order and in
-- the columns import_candidate_match_album_link uses. `stated` says how:
-- 'page', its release group's page or its own links it; 'wikidata', a
-- Wikidata item that page links states it; 'release', the release
-- (`musicbrainz_release`) links a twin release (`release_catalog`,
-- `release_key`) whose own document files it under the album.
CREATE TABLE IF NOT EXISTS source_release_album_link (
    catalog             TEXT NOT NULL CHECK (catalog = 'musicbrainz'),
    release_id          TEXT NOT NULL,
    ordinal             INTEGER NOT NULL CHECK (ordinal >= 0),
    album_catalog       TEXT NOT NULL CHECK (album_catalog <> '' AND album_catalog <> 'musicbrainz'),
    album_key           TEXT NOT NULL CHECK (album_key <> ''),
    stated              TEXT NOT NULL CHECK (stated IN ('page', 'wikidata', 'release')),
    wikidata_item       TEXT CHECK (wikidata_item IS NULL OR wikidata_item <> ''),
    musicbrainz_release TEXT CHECK (musicbrainz_release IS NULL OR musicbrainz_release <> ''),
    release_catalog     TEXT CHECK (release_catalog IS NULL OR release_catalog <> ''),
    release_key         TEXT CHECK (release_key IS NULL OR release_key <> ''),
    PRIMARY KEY (catalog, release_id, ordinal),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE,
    CHECK ((stated = 'wikidata') = (wikidata_item IS NOT NULL)),
    CHECK ((stated = 'release') = (musicbrainz_release IS NOT NULL)),
    CHECK ((stated = 'release') = (release_catalog IS NOT NULL)),
    CHECK ((stated = 'release') = (release_key IS NOT NULL))
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

-- Every medium of the release, in order, with its carrier (a
-- `crate::pressing::Medium` key, NULL where bae does not know it): the format
-- a MusicBrainz medium states, or for a Discogs release, whose one tracklist
-- is split into the mediums its positions name, the carrier a position prefix
-- or the release's format entries give each.
CREATE TABLE IF NOT EXISTS source_release_medium (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    medium     TEXT CHECK (medium IS NULL OR medium <> ''),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- The disc IDs MusicBrainz has registered against a medium, in its order: a
-- folder whose rip hashes to one of them is a rip of that medium. A Discogs
-- medium holds none.
CREATE TABLE IF NOT EXISTS source_release_medium_disc (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL,
    disc       INTEGER NOT NULL CHECK (disc >= 0),
    disc_id    TEXT NOT NULL CHECK (disc_id <> ''),
    PRIMARY KEY (catalog, release_id, position, disc),
    UNIQUE (catalog, release_id, position, disc_id),
    FOREIGN KEY (catalog, release_id, position)
        REFERENCES source_release_medium (catalog, release_id, position) ON DELETE CASCADE
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
    -- What the catalog prints after the credit when another follows.
    join_phrase           TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
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
-- 'discogs_not_configured' for a Discogs document with no key to ask with,
-- 'stopped_short' for one an identification read does not follow, or the
-- release's own document where the read took the release from a Disc ID
-- lookup's answer, which a complete read fetches. A new fetch replaces them.
CREATE TABLE IF NOT EXISTS source_release_unfetched (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    document   TEXT NOT NULL,
    key        TEXT NOT NULL CHECK (key <> ''),
    reason     TEXT NOT NULL CHECK (reason IN ('failed', 'discogs_not_configured', 'stopped_short')),
    PRIMARY KEY (catalog, release_id, document, key),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- Profiles retain the stable member id even after membership removal.
-- Only coven writes file hashes and locations; no avatar means no file.
CREATE TABLE people (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    private_circle TEXT NOT NULL,
    history_sharing TEXT NOT NULL CHECK (history_sharing IN ('private', 'household')),
    avatar_id TEXT,
    file_size INTEGER,
    hash BLOB,
    location TEXT
) STRICT;
-- Profiles list by name.
CREATE INDEX people_name ON people(name, id);

-- Ratings identify one person's judgment of an album or one release's track.
CREATE TABLE ratings (
    id TEXT PRIMARY KEY NOT NULL,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    album_id TEXT REFERENCES albums(id) ON DELETE CASCADE,
    track_id TEXT REFERENCES tracks(id) ON DELETE CASCADE,
    stars INTEGER NOT NULL CHECK (stars BETWEEN 1 AND 5),
    CHECK ((album_id IS NULL) <> (track_id IS NULL)),
    UNIQUE (person_id, album_id),
    UNIQUE (person_id, track_id)
) STRICT;

CREATE TABLE playlists (
    id TEXT PRIMARY KEY NOT NULL,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    audience TEXT NOT NULL
) STRICT;
-- Playlists list by name.
CREATE INDEX playlists_name ON playlists(name, id);

-- The position id distinguishes repeated tracks and concurrent insertions.
-- Integer order is not unique: ties retain both entries, ordered by position id.
CREATE TABLE playlist_entries (
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    position_id TEXT NOT NULL,
    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (playlist_id, position_id)
) STRICT;
-- A playlist's entries read in order, and its last position found for an
-- append, along the playlist's own run of this index.
CREATE INDEX playlist_entries_position ON playlist_entries(playlist_id, position, position_id);

CREATE TABLE history (
    id TEXT PRIMARY KEY NOT NULL,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    played_ms INTEGER NOT NULL CHECK (played_ms > 0),
    audience TEXT NOT NULL
) STRICT;
CREATE INDEX history_started_at ON history(started_at);

-- Before storage establishes a person, listening stays private on this device.
-- Creating the profile transfers these plays to its private circle atomically.
CREATE TABLE local_history (
    id TEXT PRIMARY KEY NOT NULL,
    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    played_ms INTEGER NOT NULL CHECK (played_ms > 0)
) STRICT;
