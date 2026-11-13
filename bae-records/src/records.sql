-- The library's records: bae's tables of artists, albums, releases and
-- tracks with their credits, labels, files and audio. Part of the schema
-- coven applies as host migration 1.

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

CREATE INDEX IF NOT EXISTS idx_artists_name_key ON artists (name_key);

CREATE INDEX IF NOT EXISTS idx_artists_discogs_id ON artists (discogs_artist_id);

CREATE INDEX IF NOT EXISTS idx_artists_mb_id ON artists (musicbrainz_artist_id);

-- An artist's other printed names: a name a credit prints for it that is not
-- its own (`artists.name`). A row is keyed by the artist and the folded name,
-- so one printed name of an artist is one row.
CREATE TABLE IF NOT EXISTS artist_credits (
    artist_id TEXT NOT NULL,
    name_key TEXT NOT NULL,
    name TEXT NOT NULL CHECK (name <> ''),
    created_at TEXT NOT NULL,
    PRIMARY KEY (artist_id, name_key),
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

-- Two library artists the user confirmed are one: `id` was absorbed into
-- `into_artist_id`. Both rows stay so credits naming the absorbed artist still
-- resolve; reads go through `artist_shown_as`.
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

-- An artist arrives showing as itself: a merge names both its artists, so
-- none naming it is written or applied before it.
CREATE TRIGGER IF NOT EXISTS artist_added AFTER INSERT ON artists
BEGIN
    INSERT INTO artist_shown_as (artist_id, shown_artist_id) VALUES (NEW.id, NEW.id);
END;

-- Inserting a merge's two artists into `artist_merge_refresh` sets the shown
-- artist of every artist joined to either. The view holds no rows: it names
-- the statement its INSTEAD OF trigger runs, which the two triggers on
-- `artist_merges` share. No write deletes a merge, so none follows a delete.
CREATE VIEW IF NOT EXISTS artist_merge_refresh AS SELECT NULL AS artist_id, NULL AS into_artist_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS artist_merge_refresh INSTEAD OF INSERT ON artist_merge_refresh
BEGIN
    UPDATE artist_shown_as
    SET shown_artist_id = COALESCE(
        (SELECT survivor_id FROM merged_artist_survivors WHERE artist_id = artist_shown_as.artist_id),
        artist_shown_as.artist_id)
    WHERE artist_id IN (NEW.artist_id, NEW.into_artist_id)
       OR artist_id IN (SELECT member_id FROM artist_merge_groups WHERE artist_id IN (NEW.artist_id, NEW.into_artist_id));
END;

CREATE TRIGGER IF NOT EXISTS artist_merge_added AFTER INSERT ON artist_merges
BEGIN
    INSERT INTO artist_merge_refresh (artist_id, into_artist_id) VALUES (NEW.id, NEW.into_artist_id);
END;

CREATE TRIGGER IF NOT EXISTS artist_merge_changed AFTER UPDATE OF into_artist_id ON artist_merges
BEGIN
    INSERT INTO artist_merge_refresh (artist_id, into_artist_id) VALUES (OLD.id, OLD.into_artist_id);
    INSERT INTO artist_merge_refresh (artist_id, into_artist_id) VALUES (NEW.id, NEW.into_artist_id);
END;

-- One stored picture per artist; `id` is the artist's id. `file` is the
-- picture's coven file (`coven_files`), which a new picture replaces and two
-- merged artists share.
CREATE TABLE IF NOT EXISTS artist_images (
    id TEXT PRIMARY KEY,
    file TEXT NOT NULL,
    content_type TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

-- An artist image's row by its file, which coven's pending records name.
CREATE INDEX IF NOT EXISTS idx_artist_images_file ON artist_images (file);

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
    -- What the artist did on the work: a `bae_types::credit_role::CreditRoleKind`
    -- key, or 'stated' for a role in the catalog's own words (`role_words`).
    role TEXT NOT NULL CHECK (role IN ('writer', 'composer', 'lyricist', 'librettist', 'translator', 'reviser', 'scriptwriter', 'reconstructor', 'arranger', 'producer', 'engineer', 'mixer', 'mastering', 'performer', 'instrument', 'voice', 'stated')),
    role_words TEXT CHECK (role_words IS NULL OR role_words <> ''),
    -- The role's qualifiers, a JSON list (`bae_types::credit_role::RoleQualifier`).
    qualifiers TEXT NOT NULL CHECK (json_valid(qualifiers) AND json_type(qualifiers) = 'array'),
    -- One of the artist's other printed names (`artist_credits`) this credit
    -- prints; NULL prints the artist's own.
    credit_key TEXT,
    created_at TEXT NOT NULL,
    CHECK (CASE role WHEN 'stated' THEN role_words IS NOT NULL WHEN 'instrument' THEN role_words IS NOT NULL WHEN 'voice' THEN role_words IS NOT NULL WHEN 'performer' THEN 1 ELSE role_words IS NULL END),
    FOREIGN KEY (work_id) REFERENCES works (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id, credit_key) REFERENCES artist_credits (artist_id, name_key),
    UNIQUE(work_id, artist_id, position)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_work_artists_credit ON work_artists (artist_id, credit_key);

-- The roles that make an artist a composer of a work: the writing roles
-- (`bae_types::credit_role::CreditRoleKind::is_writing`), as `work_artists.role`
-- holds them.
CREATE VIEW IF NOT EXISTS writing_credit_roles AS
SELECT 'writer' AS role
UNION ALL SELECT 'composer'
UNION ALL SELECT 'lyricist'
UNION ALL SELECT 'librettist'
UNION ALL SELECT 'translator'
UNION ALL SELECT 'reviser'
UNION ALL SELECT 'scriptwriter'
UNION ALL SELECT 'reconstructor';

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
    -- Where the pressing was released: an ISO 3166-1 alpha-2 country code, or a
    -- `crate::pressing::Region` key for a region no current code names.
    country            TEXT CHECK (country IS NULL OR (length(country) = 2 AND country = upper(country))),
    region             TEXT CHECK (region IS NULL OR region <> ''),
    -- Its labels are its `release_labels` rows, its barcodes its
    -- `release_barcodes` rows, its media its `release_media` rows.
    status             TEXT CHECK (status IS NULL OR status IN ('official', 'promotion', 'bootleg', 'pseudo_release', 'withdrawn', 'expunged', 'cancelled')),
    packaging          TEXT CHECK (packaging IS NULL OR packaging IN ('jewel_case', 'slim_jewel_case', 'digipak', 'cardboard_sleeve', 'other', 'keep_case', 'unpackaged', 'cassette_case', 'book', 'fatbox', 'snap_case', 'gatefold_cover', 'discbox_slider', 'super_jewel_box', 'digibook', 'plastic_sleeve', 'box', 'slidepack', 'snap_pack', 'metal_tin', 'longbox', 'clamshell_case', 'digifile', 'slipcase')),
    -- What Discogs says about the pressing that no column holds: a JSON array
    -- of `crate::pressing::DiscogsDetail` keys, each once, in its order.
    discogs_details    TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(discogs_details) AND json_type(discogs_details) = 'array'),
    -- The name of the folder the release was imported from, as
    -- `coven::path_text` shows it: the folder an export recreates. One name,
    -- never empty (a path of no names is no folder), `/` included.
    source_folder_name TEXT CHECK (source_folder_name <> ''),
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

-- Every record label the library knows. A label is found by a catalog id it
-- carries first, then by `name_key`: `name` read by
-- `text_match::label_name_key` (its words run together, without the trade
-- words it ends on), always written together with `name`.
CREATE TABLE IF NOT EXISTS labels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (name <> ''),
    name_key TEXT NOT NULL,
    musicbrainz_label_id TEXT CHECK (musicbrainz_label_id IS NULL OR musicbrainz_label_id <> ''),
    discogs_label_id TEXT CHECK (discogs_label_id IS NULL OR discogs_label_id <> ''),
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_labels_name_key ON labels (name_key);

CREATE INDEX IF NOT EXISTS idx_labels_musicbrainz_id ON labels (musicbrainz_label_id);

CREATE INDEX IF NOT EXISTS idx_labels_discogs_id ON labels (discogs_label_id);

-- Two library labels the person confirmed are one: `id` was absorbed into
-- `into_label_id`. Both rows stay so a release naming the absorbed label still
-- resolves; reads go through `merged_label_survivors`.
CREATE TABLE IF NOT EXISTS label_merges (
    id TEXT PRIMARY KEY,
    into_label_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    CHECK (id <> into_label_id),
    FOREIGN KEY (id) REFERENCES labels (id) ON DELETE CASCADE,
    FOREIGN KEY (into_label_id) REFERENCES labels (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_label_merges_into ON label_merges (into_label_id);

-- Each merged label and the label it shows as: the end of its merge chain. A
-- cycle, left by devices merging a pair in opposite directions, shows as its
-- smallest id. A label absent here shows as itself.
CREATE VIEW IF NOT EXISTS merged_label_survivors AS
WITH RECURSIVE hop(start, current, depth) AS (
    SELECT id, into_label_id, 1 FROM label_merges
    UNION ALL
    SELECT hop.start, merge.into_label_id, hop.depth + 1
    FROM hop JOIN label_merges merge ON merge.id = hop.current
    WHERE hop.depth < 64
),
resolved(label_id, survivor_id) AS (
    SELECT start,
           COALESCE(
               MIN(CASE WHEN current NOT IN (SELECT id FROM label_merges) THEN current END),
               MIN(current)
           )
    FROM hop
    GROUP BY start
)
SELECT label_id, survivor_id FROM resolved WHERE label_id <> survivor_id;

-- Every label a pressing is on, in its source's order, each naming a library
-- label, the catalog number that label gives the pressing, or both. The id
-- derives from (release, position): one place in a release's labels is one row.
CREATE TABLE IF NOT EXISTS release_labels (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    -- Counted from zero.
    position INTEGER NOT NULL CHECK (position >= 0),
    label_id TEXT,
    catalog_number TEXT CHECK (catalog_number IS NULL OR catalog_number <> ''),
    UNIQUE (release_id, position),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE,
    FOREIGN KEY (label_id) REFERENCES labels (id) ON DELETE CASCADE,
    CHECK (label_id IS NOT NULL OR catalog_number IS NOT NULL)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_labels_label ON release_labels (label_id);

-- Every barcode a pressing is printed with, in its source's order. The id
-- derives from (release, position): one place in a release's barcodes is one
-- row.
CREATE TABLE IF NOT EXISTS release_barcodes (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    -- Counted from zero.
    position INTEGER NOT NULL CHECK (position >= 0),
    barcode TEXT NOT NULL CHECK (barcode <> ''),
    UNIQUE (release_id, position),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

-- The carriers a pressing is made of, each with how many of it there are, in
-- the record's order: two CDs, one vinyl record. The id derives from
-- (release, position): one place in a release's media is one row.
CREATE TABLE IF NOT EXISTS release_media (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    -- Counted from zero.
    position INTEGER NOT NULL CHECK (position >= 0),
    -- A `crate::pressing::Medium` key.
    medium TEXT NOT NULL CHECK (medium <> ''),
    count INTEGER NOT NULL CHECK (count >= 1),
    UNIQUE (release_id, position),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

-- One stored cover per release; `id` is the release's id. `file` is the
-- cover's coven file (`coven_files`), which a new cover replaces.
CREATE TABLE IF NOT EXISTS covers (
    id TEXT PRIMARY KEY,
    file TEXT NOT NULL,
    content_type TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

-- A cover's row by its file, which coven's pending records name.
CREATE INDEX IF NOT EXISTS idx_covers_file ON covers (file);

-- The files a release's audio and artwork are stored as, with the facts of the
-- audio they were imported from.
CREATE TABLE IF NOT EXISTS release_files (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    -- The file's names in its release joined by `/`, none of them empty: a
    -- name, or one that begins, ends or holds a doubled `/`, names no file.
    original_filename TEXT NOT NULL CHECK (
        original_filename <> ''
        AND substr(original_filename, 1, 1) <> '/'
        AND substr(original_filename, -1) <> '/'
        AND instr(original_filename, '//') = 0
    ),
    content_type TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- The file's coven file (`coven_files`), which holds its size, its hash
    -- and where it is: on the device that imported it, or in the cloud.
    file TEXT NOT NULL,
    -- What the file is to its release, as the import's scan read it. Its
    -- content type can't say: a damaged image is stored as plain bytes, and
    -- a CUE sheet, a rip log and a playlist all as text. Audio, and only
    -- audio, states the facts below.
    role TEXT NOT NULL CHECK (role IN (
        'audio', 'track_sheet', 'artwork', 'damaged_image', 'document', 'other'
    )),
    -- Which document the file is, as the scan read it; set for a document,
    -- and only a document.
    document_kind TEXT CHECK (document_kind IS NULL OR document_kind IN (
        'cue_sheet', 'rip_log', 'accuraterip_report', 'playlist', 'text'
    )),
    -- How the file gives the release its tracks, its bytes stored as they
    -- were: as its own audio ('file'), or as a CUE-described image whose
    -- tracks are slices of it ('cue'). NULL when it gives no tracks.
    source_audio_layout TEXT,
    source_audio_content_type TEXT,
    source_audio_container TEXT,
    source_audio_duration_ms INTEGER CHECK (source_audio_duration_ms IS NULL OR source_audio_duration_ms >= 0),
    source_audio_sample_rate_hz INTEGER CHECK (source_audio_sample_rate_hz IS NULL OR source_audio_sample_rate_hz > 0),
    source_audio_bits_per_sample INTEGER CHECK (source_audio_bits_per_sample IS NULL OR source_audio_bits_per_sample > 0),
    source_audio_bitrate_kbps INTEGER CHECK (source_audio_bitrate_kbps IS NULL OR source_audio_bitrate_kbps > 0),
    source_audio_channels INTEGER,
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE,
    CHECK ((role = 'audio') = (source_audio_content_type IS NOT NULL)),
    CHECK ((role = 'document') = (document_kind IS NOT NULL)),
    -- A file that is not audio states no audio facts. An audio file states
    -- them all, the codec deciding whether bits per sample or bitrate is set,
    -- and a layout unless the release carries it with the tracklist leaving
    -- it out.
    CHECK (
        (source_audio_layout IS NULL
            AND source_audio_content_type IS NULL
            AND source_audio_container IS NULL
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
            AND source_audio_container IS NOT NULL
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

-- Each release's files in natural order of their names (`natural_order`:
-- "Disc 2" before "Disc 10", case aside), id breaking a tie: the order every
-- list of a release's files shows, read straight from this index.
CREATE INDEX IF NOT EXISTS idx_release_files_release_order
    ON release_files (release_id, original_filename COLLATE natural_order, id);

-- A release file's row by its coven file, which coven's views and pending
-- records name.
CREATE INDEX IF NOT EXISTS idx_release_files_file ON release_files (file);

-- Each release's files by kind (content type), name and id breaking a tie:
-- another order the Storage sheet's file table sorts by, read from its index
-- either way. Its order by size sorts one release's few dozen files by their
-- coven files' sizes, with no index.
CREATE INDEX IF NOT EXISTS idx_release_files_release_kind
    ON release_files (release_id, content_type, original_filename COLLATE natural_order, id);

-- Who is credited on a release, and in what role.
CREATE TABLE IF NOT EXISTS release_artist_roles (
    id TEXT PRIMARY KEY,
    release_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    -- What the artist did: a `bae_types::credit_role::CreditRoleKind` key,
    -- 'performer' (its part in the catalog's words, if the catalog names one
    -- bae's lists lack, in `role_words`), 'instrument' (the id of an instrument on
    -- MusicBrainz's list in `role_words`), 'voice' (the id of a voice on MusicBrainz's list in `role_words`), or
    -- 'stated' for the catalog's own words (`role_words`), with the role's
    -- qualifiers as a JSON list.
    role TEXT NOT NULL CHECK (role IN ('writer', 'composer', 'lyricist', 'librettist', 'translator', 'reviser', 'scriptwriter', 'reconstructor', 'arranger', 'producer', 'engineer', 'mixer', 'mastering', 'performer', 'instrument', 'voice', 'stated')),
    role_words TEXT CHECK (role_words IS NULL OR role_words <> ''),
    qualifiers TEXT NOT NULL CHECK (json_valid(qualifiers) AND json_type(qualifiers) = 'array'),
    -- One of the artist's other printed names (`artist_credits`) this credit
    -- prints; NULL prints the artist's own.
    credit_key TEXT,
    created_at TEXT NOT NULL,
    CHECK (CASE role WHEN 'stated' THEN role_words IS NOT NULL WHEN 'instrument' THEN role_words IS NOT NULL WHEN 'voice' THEN role_words IS NOT NULL WHEN 'performer' THEN 1 ELSE role_words IS NULL END),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id, credit_key) REFERENCES artist_credits (artist_id, name_key),
    UNIQUE(release_id, artist_id, position, source)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_artist_roles_credit ON release_artist_roles (artist_id, credit_key);

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
    kind        TEXT NOT NULL CHECK (kind IN ('pressing', 'album')),
    CHECK (kind = 'pressing' OR album_key IS NULL),
    UNIQUE (release_id, catalog),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_records_catalog_key ON release_records (catalog, key) WHERE kind = 'pressing';

-- Each release's records in the order surfaces list catalogs
-- (`bae_catalog_rank`), which the release's detail reads them in.
CREATE INDEX IF NOT EXISTS idx_release_records_release_order
    ON release_records (release_id, bae_catalog_rank(catalog));

CREATE INDEX IF NOT EXISTS idx_release_records_catalog_album ON release_records
    (catalog, CASE WHEN kind = 'album' THEN key ELSE album_key END);

-- The mediums a release holds — the discs, records or tapes its audio was
-- read from — in order. The id derives from (release, position), so two
-- devices that write one release's mediums write the same rows.
CREATE TABLE IF NOT EXISTS release_mediums (
    id TEXT NOT NULL,
    release_id TEXT NOT NULL,
    -- Counted from one.
    position INTEGER NOT NULL CHECK (position >= 1),
    -- The carrier (a `crate::pressing::Medium` key), where the source states one.
    medium TEXT CHECK (medium IS NULL OR medium <> ''),
    -- A CD is one side; a record or a cassette as many as its tracks name.
    side_count INTEGER NOT NULL CHECK (side_count >= 1),
    created_at TEXT NOT NULL,
    UNIQUE (release_id, position),
    -- The key includes the release so a track's (medium, release) pair names
    -- it: a track's medium is one of its own release's.
    PRIMARY KEY (id, release_id),
    -- A medium id still names one medium on its own.
    UNIQUE (id),
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
    FOREIGN KEY (medium_id, release_id) REFERENCES release_mediums (id, release_id)
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

-- Who is credited on a track, and in what role.
CREATE TABLE IF NOT EXISTS track_artist_roles (
    id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    source TEXT NOT NULL,
    -- What the artist did: a `bae_types::credit_role::CreditRoleKind` key,
    -- 'performer' (its part in the catalog's words, if the catalog names one
    -- bae's lists lack, in `role_words`), 'instrument' (the id of an instrument on
    -- MusicBrainz's list in `role_words`), 'voice' (the id of a voice on MusicBrainz's list in `role_words`), or
    -- 'stated' for the catalog's own words (`role_words`), with the role's
    -- qualifiers as a JSON list.
    role TEXT NOT NULL CHECK (role IN ('writer', 'composer', 'lyricist', 'librettist', 'translator', 'reviser', 'scriptwriter', 'reconstructor', 'arranger', 'producer', 'engineer', 'mixer', 'mastering', 'performer', 'instrument', 'voice', 'stated')),
    role_words TEXT CHECK (role_words IS NULL OR role_words <> ''),
    qualifiers TEXT NOT NULL CHECK (json_valid(qualifiers) AND json_type(qualifiers) = 'array'),
    -- One of the artist's other printed names (`artist_credits`) this credit
    -- prints; NULL prints the artist's own.
    credit_key TEXT,
    created_at TEXT NOT NULL,
    CHECK (CASE role WHEN 'stated' THEN role_words IS NOT NULL WHEN 'instrument' THEN role_words IS NOT NULL WHEN 'voice' THEN role_words IS NOT NULL WHEN 'performer' THEN 1 ELSE role_words IS NULL END),
    FOREIGN KEY (track_id) REFERENCES tracks (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id, credit_key) REFERENCES artist_credits (artist_id, name_key),
    UNIQUE(track_id, artist_id, position, source)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_track_artist_roles_credit ON track_artist_roles (artist_id, credit_key);

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
    -- Sample counts: never negative.
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
    -- Positions within the file: never negative.
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

-- A track's audio has one body, the span from its INDEX 01 on, and at most
-- one pregap before it, which can lie in the file before the body's (§2.2).
-- Each format's segments are written with the format, in one write.
CREATE UNIQUE INDEX IF NOT EXISTS idx_audio_format_segments_one_main
    ON audio_format_segments (audio_format_id) WHERE role = 'main';
CREATE UNIQUE INDEX IF NOT EXISTS idx_audio_format_segments_one_pregap
    ON audio_format_segments (audio_format_id) WHERE role = 'audio_pregap';

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
