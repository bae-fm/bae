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

-- An artist arriving takes its shown artist: the survivor of its merges,
-- else itself.
CREATE TRIGGER IF NOT EXISTS artist_added AFTER INSERT ON artists
BEGIN
    INSERT INTO artist_shown_as (artist_id, shown_artist_id)
    VALUES (NEW.id, COALESCE((SELECT survivor_id FROM merged_artist_survivors WHERE artist_id = NEW.id), NEW.id));
END;

-- Inserting a merge's two artists into `artist_merge_refresh` sets the shown
-- artist of every artist joined to either. The view holds no rows: it names
-- the statement its INSTEAD OF trigger runs, which the three triggers on
-- `artist_merges` share.
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

CREATE TRIGGER IF NOT EXISTS artist_merge_changed AFTER UPDATE OF id, into_artist_id ON artist_merges
BEGIN
    INSERT INTO artist_merge_refresh (artist_id, into_artist_id) VALUES (OLD.id, OLD.into_artist_id);
    INSERT INTO artist_merge_refresh (artist_id, into_artist_id) VALUES (NEW.id, NEW.into_artist_id);
END;

CREATE TRIGGER IF NOT EXISTS artist_merge_removed AFTER DELETE ON artist_merges
BEGIN
    INSERT INTO artist_merge_refresh (artist_id, into_artist_id) VALUES (OLD.id, OLD.into_artist_id);
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
    -- `coven::path_text` shows it: the folder an export recreates.
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
-- `into_label_id`. Both rows stay so a release another device gave the
-- absorbed label still names it; reads go through `merged_label_survivors`.
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
-- derives from (release, position), so two devices that write one release's
-- labels write the same rows.
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
-- derives from (release, position), so two devices that write one release's
-- barcodes write the same rows.
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
-- (release, position), so two devices that write one release's media write
-- the same rows.
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
    original_filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    created_at TEXT NOT NULL,
    -- The file's coven file (`coven_files`), which holds its size, its hash
    -- and where it is: on the device that imported it, or in the cloud.
    file TEXT NOT NULL,
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

-- Where each release's files are on this device: their count and total
-- size; `stored`, 1 when it has files and every one names a coven file
-- `coven_files` records as stored; `pinned_count`, how many of them coven's
-- `coven_pins` lists as kept whole on this device; and `queued_count` and
-- `first_queued_position`, how many of them coven's `coven_upload_queue`
-- lists and the place of the first (NULL with none). One row per release,
-- written by the triggers below in the write that changes its files, their
-- coven records, pins or queue: this device's own, another device's applied,
-- or coven's. A file whose release has not arrived moves nothing; the
-- release's own trigger counts it when it does.
CREATE TABLE IF NOT EXISTS release_storage (
    release_id TEXT PRIMARY KEY REFERENCES releases (id) ON DELETE CASCADE,
    file_count INTEGER NOT NULL,
    total_size INTEGER NOT NULL,
    stored INTEGER NOT NULL,
    pinned_count INTEGER NOT NULL,
    queued_count INTEGER NOT NULL,
    first_queued_position INTEGER,
    CHECK ((queued_count = 0) = (first_queued_position IS NULL))
) STRICT;

-- The releases with files waiting in this device's upload queue, in the order
-- their first files wait: what the import list's Done tab runs first (§3.2).
CREATE INDEX IF NOT EXISTS idx_release_storage_uploading ON release_storage (first_queued_position, release_id)
    WHERE first_queued_position IS NOT NULL;

-- Inserting a release id into `release_storage_refresh` rewrites that
-- release's row from its files and coven's records of them. The view holds no
-- rows; it names the statement its INSTEAD OF trigger runs.
CREATE VIEW IF NOT EXISTS release_storage_refresh AS SELECT NULL AS release_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS release_storage_refresh INSTEAD OF INSERT ON release_storage_refresh
BEGIN
    INSERT INTO release_storage (
        release_id, file_count, total_size, stored, pinned_count, queued_count,
        first_queued_position)
    SELECT r.id,
           (SELECT COUNT(*) FROM release_files f WHERE f.release_id = r.id),
           (SELECT COALESCE(SUM(coven.size), 0) FROM release_files f
            JOIN coven_files coven ON coven.id = f.file
            WHERE f.release_id = r.id),
           EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = r.id)
             AND NOT EXISTS (SELECT 1 FROM release_files f
                             LEFT JOIN coven_files coven ON coven.id = f.file
                             WHERE f.release_id = r.id AND coven.hash IS NULL),
           (SELECT COUNT(*) FROM release_files f
            JOIN coven_pins pin ON pin.file = f.file
            WHERE f.release_id = r.id),
           (SELECT COUNT(*) FROM release_files f
            JOIN coven_upload_queue queued ON queued.file = f.file
            WHERE f.release_id = r.id),
           (SELECT MIN(queued.position) FROM release_files f
            JOIN coven_upload_queue queued ON queued.file = f.file
            WHERE f.release_id = r.id)
    FROM releases r
    WHERE r.id = NEW.release_id
    ON CONFLICT (release_id) DO UPDATE SET
        file_count = excluded.file_count,
        total_size = excluded.total_size,
        stored = excluded.stored,
        pinned_count = excluded.pinned_count,
        queued_count = excluded.queued_count,
        first_queued_position = excluded.first_queued_position;
END;

-- A release arriving counts the files, coven records, pins and queued files
-- already there for it, which a write applied from another device can hold
-- before the release.
CREATE TRIGGER IF NOT EXISTS release_storage_release_added AFTER INSERT ON releases
BEGIN
    INSERT INTO release_storage_refresh (release_id) VALUES (NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS release_storage_file_added AFTER INSERT ON release_files
BEGIN
    INSERT INTO release_storage_refresh (release_id) VALUES (NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS release_storage_file_changed AFTER UPDATE OF release_id, file ON release_files
BEGIN
    INSERT INTO release_storage_refresh (release_id) VALUES (OLD.release_id);
    INSERT INTO release_storage_refresh (release_id) VALUES (NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS release_storage_file_removed AFTER DELETE ON release_files
BEGIN
    INSERT INTO release_storage_refresh (release_id) VALUES (OLD.release_id);
END;

-- A release file's coven file being recorded, switched to its stored bytes
-- or deleted refreshes its release: whether the release is stored follows
-- its files' records, which coven changes with no change to a row of bae's,
-- in this device's write completing an upload and as another device's write
-- applies. A record no release file names moves nothing.
CREATE TRIGGER IF NOT EXISTS release_storage_coven_file_added AFTER INSERT ON coven_files
BEGIN
    INSERT INTO release_storage_refresh (release_id)
    SELECT release_id FROM release_files WHERE file = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS release_storage_coven_file_switched AFTER UPDATE OF coordinates ON coven_files
BEGIN
    INSERT INTO release_storage_refresh (release_id)
    SELECT release_id FROM release_files WHERE file = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS release_storage_coven_file_removed AFTER DELETE ON coven_files
BEGIN
    INSERT INTO release_storage_refresh (release_id)
    SELECT release_id FROM release_files WHERE file = OLD.id;
END;

-- A release file's pin moves its release's pinned count, in the transaction
-- that changes the pin: this device's own write, another device's applied,
-- or coven's own as a file is pinned, kept here or unpinned. A pin is listed
-- only once the whole file is on this device. A file no release file names,
-- or whose release has not arrived, moves nothing: the refresh above counts
-- its release's pins from `coven_pins` as it writes the release's row.
CREATE TRIGGER IF NOT EXISTS release_storage_file_pinned AFTER INSERT ON coven_pins
BEGIN
    UPDATE release_storage SET pinned_count = pinned_count + 1
    WHERE release_id = (SELECT release_id FROM release_files WHERE file = NEW.file);
END;

CREATE TRIGGER IF NOT EXISTS release_storage_file_unpinned AFTER DELETE ON coven_pins
BEGIN
    UPDATE release_storage SET pinned_count = pinned_count - 1
    WHERE release_id = (SELECT release_id FROM release_files WHERE file = OLD.file);
END;

-- A release file joining or leaving this device's upload queue moves its
-- release's queued count and first place, in the transaction that changes
-- the queue: staging the file for the cloud, its move switching it to the
-- cloud, its upload retiring, or the row no longer naming it. A file no
-- release file names, or whose release has not arrived, moves nothing: the
-- refresh above counts its release's queued files from `coven_upload_queue`
-- as it writes the release's row.
CREATE TRIGGER IF NOT EXISTS release_storage_file_queued AFTER INSERT ON coven_upload_queue
BEGIN
    UPDATE release_storage SET queued_count = queued_count + 1,
        first_queued_position = MIN(COALESCE(first_queued_position, NEW.position), NEW.position)
    WHERE release_id = (SELECT release_id FROM release_files WHERE file = NEW.file);
END;

CREATE TRIGGER IF NOT EXISTS release_storage_file_unqueued AFTER DELETE ON coven_upload_queue
BEGIN
    UPDATE release_storage SET queued_count = queued_count - 1,
        first_queued_position = CASE WHEN first_queued_position = OLD.position
            THEN (SELECT MIN(queued.position) FROM release_files f
                  JOIN coven_upload_queue queued ON queued.file = f.file
                  WHERE f.release_id = release_storage.release_id)
            ELSE first_queued_position END
    WHERE release_id = (SELECT release_id FROM release_files WHERE file = OLD.file);
END;

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

-- A track's audio has one body, the span from its INDEX 01 on, and at most
-- one pregap before it, which can lie in the file before the body's (§2.2).
-- Each format's segments are written with the format, in one write, so no
-- merge judges two devices' claims on one of these.
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

-- ── Playback ──────────────────────────────────────────────────────────────────

-- What this device plays next, and how. One row; never synced. Up Next is
-- `playback_manual`, the context lane `playback_context`.
CREATE TABLE IF NOT EXISTS playback_queue (
    singleton  INTEGER PRIMARY KEY CHECK (singleton = 1),
    repeat     TEXT NOT NULL CHECK (repeat IN ('off', 'track', 'context')),
    -- The entry number the next entry of either lane gets: no two entries
    -- ever share one, so an id a screen still holds never names a later row.
    next_entry INTEGER NOT NULL,
    -- What the context lane plays from (one release, several, a list of
    -- tracks, or the library), whether it plays shuffled, and the position of
    -- its track playing or last played: the lane's upcoming tracks are the
    -- ones after it. NULL together exactly while the lane is empty. Which
    -- releases several were, or which tracks a list named, is the lane's
    -- tracks.
    source_kind       TEXT CHECK (source_kind IN ('release', 'releases', 'tracks', 'library')),
    -- The release the lane plays, set exactly when it plays one.
    source_release_id TEXT,
    shuffled          INTEGER CHECK (shuffled IN (0, 1)),
    cursor            INTEGER,
    -- How many times tracks went into Up Next since playback started with
    -- the library, and how many tracks the last of those put in, NULL before
    -- the first. The write that adds the tracks counts them here, so a
    -- screen confirming each addition reads one value per addition, even
    -- two alike in a row. Starting playback with the library zeroes both.
    additions    INTEGER NOT NULL CHECK (additions >= 0),
    added_tracks INTEGER CHECK (added_tracks > 0),
    -- How many writes have put entries into Up Next, taken them out or moved
    -- them; and how many have changed the context lane's tracks after its
    -- playing one: put them in, taken them out, reordered them, or moved
    -- which track is playing. Each write that does so counts it here, so a
    -- queue screen holding a row it moved knows when the lane's read shows
    -- the move, and its rows know when to read again.
    manual_version  INTEGER NOT NULL CHECK (manual_version >= 0),
    context_version INTEGER NOT NULL CHECK (context_version >= 0),
    CHECK ((source_kind IS NULL) = (shuffled IS NULL)
        AND (source_kind IS NULL) = (cursor IS NULL)),
    CHECK ((source_kind IS 'release') = (source_release_id IS NOT NULL)),
    CHECK ((additions = 0) = (added_tracks IS NULL))
) STRICT;

INSERT OR IGNORE INTO playback_queue (
    singleton, repeat, next_entry, additions, manual_version, context_version)
VALUES (1, 'off', 1, 0, 0, 0);

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

-- The context lane a play fills, in position order. Each row is an entry of
-- its own, as in Up Next, so a track named twice is two rows. A play numbers
-- its entries in track order and puts them in in that order, so the table and
-- its track index grow at their end and only the position index takes rows
-- out of order. Positions are only an order, with gaps where rows left. No
-- trigger runs on its rows: a play takes out and puts in up to the whole
-- library's, so a trigger would run once for each, and SQLite empties a table
-- with a trigger row by row. Each write that leaves the lane empty clears
-- what it played from itself. Never synced.
CREATE TABLE IF NOT EXISTS playback_context (
    entry      INTEGER PRIMARY KEY,
    position   INTEGER NOT NULL UNIQUE,
    track_id   TEXT NOT NULL,
    -- The row's position in the order unshuffling puts the lane back in: its
    -- order when shuffle turned on, or the source's order for a shuffled play.
    -- Read only while the lane is shuffled.
    unshuffled INTEGER NOT NULL
) STRICT;
-- A deleted track's rows are found by its id.
CREATE INDEX IF NOT EXISTS playback_context_track ON playback_context(track_id);

-- A deleted track leaves the queue in the same transaction. A trigger rather
-- than a foreign key: the queue's rows are only ever made from tracks the
-- library holds, so a key would only look each one up again as a play fills
-- the lane.
CREATE TRIGGER IF NOT EXISTS tracks_leave_playback_queue AFTER DELETE ON tracks
BEGIN
    UPDATE playback_queue SET manual_version = manual_version + 1
    WHERE EXISTS (SELECT 1 FROM playback_manual WHERE track_id = OLD.id);
    UPDATE playback_queue SET context_version = context_version + 1
    WHERE EXISTS (SELECT 1 FROM playback_context WHERE track_id = OLD.id);
    DELETE FROM playback_manual WHERE track_id = OLD.id;
    DELETE FROM playback_context WHERE track_id = OLD.id;
    -- The lane's last track deleted takes what it played from with it.
    UPDATE playback_queue
    SET source_kind = NULL, source_release_id = NULL, shuffled = NULL, cursor = NULL
    WHERE source_kind IS NOT NULL AND NOT EXISTS (SELECT 1 FROM playback_context);
END;

-- Where this device's playback stood, so a restart resumes it: the queue
-- entry playing and how far into its stream. The volume it resumes at is
-- `playback_now`'s. Present while a track is loaded; never synced.
CREATE TABLE IF NOT EXISTS playback_state (
    id               TEXT PRIMARY KEY,
    -- The entry id the queue screens show ('m' and an Up Next entry number,
    -- or 'c' and a context track). Read only with its track: a deleted track
    -- leaves nothing to resume.
    current_entry    TEXT,
    current_track_id TEXT REFERENCES tracks(id) ON DELETE SET NULL,
    position_ms      INTEGER CHECK (position_ms >= 0)
) STRICT;

-- What this device's playback is doing, as its screens show it: one row,
-- written by playback as each of these changes. Where it is in what it plays
-- is not here, since that moves with the audio, and its repeat mode is the
-- queue's. Starting playback with the library resets the row to stopped,
-- keeping the volume and mute only when it resumes what it played. Never
-- synced.
CREATE TABLE IF NOT EXISTS playback_now (
    singleton   INTEGER PRIMARY KEY CHECK (singleton = 1),
    state       TEXT NOT NULL CHECK (state IN ('stopped', 'loading', 'playing', 'paused')),
    -- The track the state names. A track the library no longer holds stays
    -- named until playback moves off it.
    track_id    TEXT,
    -- While loading: whether the track's audio is prepared yet. A playing or
    -- paused track always is.
    prepared    INTEGER CHECK (prepared IN (0, 1)),
    -- The prepared track's length; NULL when it is unknown.
    duration_ms INTEGER CHECK (duration_ms >= 0),
    -- Why it is paused: by hand, or at the end of a side or disc, which the
    -- pause prompt names by its boundary's id, kind and label, with when the
    -- next side starts on its own (milliseconds since the epoch), NULL when
    -- it waits for Play.
    pause_reason       TEXT CHECK (pause_reason IN ('manual', 'side_ended')),
    side_boundary_id   TEXT,
    side_boundary_kind TEXT CHECK (side_boundary_kind IN ('side', 'disc')),
    side_label         TEXT,
    side_resumes_at    INTEGER,
    volume      REAL NOT NULL CHECK (volume BETWEEN 0 AND 1),
    is_muted    INTEGER NOT NULL CHECK (is_muted IN (0, 1)),
    -- Where playback goes: this device's own output, a remote renderer, or
    -- back here after the link to the renderer was lost, with why
    -- (untranslated).
    remote             TEXT NOT NULL CHECK (remote IN ('local', 'on', 'ended')),
    remote_device_id   TEXT,
    remote_device_name TEXT,
    remote_end_detail  TEXT,
    -- The file range a preview auditions (its path as coven's bytes), and
    -- the clip's length.
    preview              TEXT NOT NULL CHECK (preview IN ('idle', 'playing', 'paused')),
    preview_path         BLOB,
    preview_start_sample INTEGER CHECK (preview_start_sample >= 0),
    preview_end_sample   INTEGER CHECK (preview_end_sample >= 0),
    preview_duration_ms  INTEGER CHECK (preview_duration_ms >= 0),
    -- Why playback last failed, until it next plays: a reason the person can
    -- act on, the other device a release's audio is on (by its name, and
    -- whether it is still in the library, removed, or replaced), or bae's own
    -- or the renderer's failure with its untranslated detail. Each failure
    -- counts up `failures`, so two alike in a row are two values a screen
    -- tells apart.
    failure         TEXT CHECK (failure IN (
        'sync_disconnected', 'on_other_device', 'original_missing',
        'audio_output', 'internal', 'renderer')),
    failure_problem TEXT CHECK (failure_problem IN (
        'device_unavailable', 'unsupported_format', 'airplay_refused')),
    failure_detail  TEXT,
    failure_device       TEXT,
    failure_device_state TEXT CHECK (failure_device_state IN ('active', 'removed', 'replaced')),
    failures        INTEGER NOT NULL CHECK (failures >= 0),
    CHECK ((state = 'stopped') = (track_id IS NULL)),
    CHECK ((state = 'loading') = (prepared IS NOT NULL)),
    CHECK (duration_ms IS NULL OR state IN ('playing', 'paused') OR prepared = 1),
    CHECK ((state = 'paused') = (pause_reason IS NOT NULL)),
    CHECK ((pause_reason IS 'side_ended') = (side_boundary_id IS NOT NULL)),
    CHECK ((side_boundary_id IS NULL) = (side_boundary_kind IS NULL)
        AND (side_boundary_id IS NULL) = (side_label IS NULL)),
    CHECK (side_resumes_at IS NULL OR side_boundary_id IS NOT NULL),
    CHECK ((remote = 'local') = (remote_device_id IS NULL)
        AND (remote = 'local') = (remote_device_name IS NULL)),
    CHECK ((remote = 'ended') = (remote_end_detail IS NOT NULL)),
    CHECK ((preview = 'idle') = (preview_path IS NULL)
        AND (preview = 'idle') = (preview_start_sample IS NULL)
        AND (preview = 'idle') = (preview_duration_ms IS NULL)),
    CHECK (preview_end_sample IS NULL OR preview_path IS NOT NULL),
    CHECK ((failure IS 'audio_output') = (failure_problem IS NOT NULL)),
    CHECK ((failure IS 'internal' OR failure IS 'renderer') = (failure_detail IS NOT NULL)),
    CHECK ((failure IS 'on_other_device') = (failure_device IS NOT NULL)
        AND (failure IS 'on_other_device') = (failure_device_state IS NOT NULL))
) STRICT;

INSERT OR IGNORE INTO playback_now (singleton, state, volume, is_muted, remote, preview, failures)
VALUES (1, 'stopped', 1.0, 0, 'local', 'idle', 0);

-- Where this device's playback is, for the surfaces that move the position on
-- themselves (the operating system's media controls, Media3): the playing
-- track's line and the preview's, each a position and whether it moves on
-- from there at the play rate. One row, written only as playback leaves a
-- line it holds: a seek, another track or clip, a pause or resume, a new
-- length, the pregap ending, or a position more than a second off the line.
-- Where playback is between those moves with the audio and is not here.
-- Starting playback with the library resets it to no line. Never synced.
CREATE TABLE IF NOT EXISTS playback_timeline (
    singleton   INTEGER PRIMARY KEY CHECK (singleton = 1),
    -- The track's line, on a timeline with no pregap: through the pregap it
    -- stands at the track's start. Set together once the playing track
    -- reports a position; the length NULL also while it is unknown.
    track_id    TEXT,
    position_ms INTEGER CHECK (position_ms >= 0),
    duration_ms INTEGER CHECK (duration_ms >= 0),
    moving      INTEGER CHECK (moving IN (0, 1)),
    -- How many seeks playback has applied since it started, so a surface
    -- that shows a seek ahead of it knows when it is applied.
    seek_revision INTEGER NOT NULL CHECK (seek_revision >= 0),
    -- The preview clip's line: the file range it auditions (its path as
    -- coven's bytes, the end NULL for the rest of the file), where it is, its
    -- length, and whether it moves on. Set together while a preview is
    -- loaded.
    preview_path         BLOB,
    preview_start_sample INTEGER CHECK (preview_start_sample >= 0),
    preview_end_sample   INTEGER CHECK (preview_end_sample >= 0),
    preview_position_ms  INTEGER CHECK (preview_position_ms >= 0),
    preview_duration_ms  INTEGER CHECK (preview_duration_ms >= 0),
    preview_moving       INTEGER CHECK (preview_moving IN (0, 1)),
    CHECK ((track_id IS NULL) = (position_ms IS NULL)
        AND (track_id IS NULL) = (moving IS NULL)),
    CHECK (duration_ms IS NULL OR track_id IS NOT NULL),
    CHECK ((preview_path IS NULL) = (preview_start_sample IS NULL)
        AND (preview_path IS NULL) = (preview_position_ms IS NULL)
        AND (preview_path IS NULL) = (preview_duration_ms IS NULL)
        AND (preview_path IS NULL) = (preview_moving IS NULL)),
    CHECK (preview_end_sample IS NULL OR preview_path IS NOT NULL)
) STRICT;

INSERT OR IGNORE INTO playback_timeline (singleton, seek_revision) VALUES (1, 0);

-- ── Cast receivers ─────────────────────────────────────────────────────────────

-- A receiver the cast picker lists: what discovery last found on the network,
-- or what a host's browser reported, merged into one list and written whole
-- in one write per change. Never synced. Discovery doesn't outlive the
-- process that ran it, so the table is emptied as the library opens.
CREATE TABLE IF NOT EXISTS cast_device (
    id   TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('cast', 'dlna', 'airplay'))
) STRICT;

-- Profiles retain the stable member id even after membership removal.
-- `avatar_id` is the profile picture's coven file (`coven_files`); NULL is
-- no picture.
CREATE TABLE people (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    private_circle TEXT NOT NULL,
    history_sharing TEXT NOT NULL CHECK (history_sharing IN ('private', 'household')),
    avatar_id TEXT
) STRICT;
-- Profiles list by name.
CREATE INDEX people_name ON people(name, id);
-- A profile by its picture's file, which coven's pending records name.
CREATE INDEX people_avatar ON people(avatar_id);

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
    started_at TEXT NOT NULL,
    played_ms INTEGER NOT NULL CHECK (played_ms > 0)
) STRICT;

-- The profile this device's person asked for while their private circle is
-- being made: the operation coven journaled for the circle (as
-- `coven_operations` lists it), the circle it makes, and the name they gave.
-- The write that journals the circle inserts it. Once the circle's entry
-- lands, the write that creates the profile from it, and moves
-- `local_history` into the circle, deletes it; so does the write that
-- acknowledges a circle the library refused. A person asks for one profile,
-- so there is at most one. Never synced.
CREATE TABLE profile_request (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    operation INTEGER NOT NULL,
    circle TEXT NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0)
) STRICT;

-- What each admin change this device asked for changes, which coven's
-- `coven_operations` doesn't say: keyed by the change's operation there, whose
-- kind says which of these columns it uses and whose outcome says how it
-- stands. A rename (`RenameStore`) has the new `name`; a role change
-- (`SetRole`) the `member` and the `role`; a person's removal
-- (`RemoveMember`) the `member`; a device's removal (`RemoveDevice`) the
-- `device`, its eight bytes as `coven_devices` holds them; an approval of a
-- sent invite's request (`Invite`, or `ReplaceMemberKeys` for a replacement
-- invite) the `member` it lets in and, as `name`, the name the invite gave
-- them, none for a replacement. The write that
-- journals the change inserts it; the write that acknowledges the change's
-- outcome deletes it. Never synced.
CREATE TABLE admin_operations (
    operation INTEGER PRIMARY KEY NOT NULL,
    member TEXT,
    device BLOB,
    role TEXT CHECK (role IN ('admin', 'member')),
    name TEXT
) STRICT;

-- Why this device's last attempt to start sync failed, while it stands: a
-- start that failed before coven's loop ran, such as a keychain locked for
-- the session, which coven's own status can't name. Written when starting
-- fails; deleted when a reconnect's start returns, and by the trigger below
-- as any start gets under way, a storage setup's among them. The sync status
-- shows it beside coven's. Emptied as the library opens: no attempt outlives
-- the process that made it. Never synced.
CREATE TABLE IF NOT EXISTS sync_start_failure (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    -- The failure's class, as the status line words it: one of the classes
    -- coven's sync errors map to, or `internal` for a start that panicked.
    category TEXT NOT NULL CHECK (category IN (
        'database', 'config', 'internal', 'sync_update_required',
        'device_reset_required', 'device_removed', 'membership', 'keyring',
        'keyring_locked', 'keychain_entitlement_missing',
        'saved_restore_codes_full', 'saved_restore_code_too_large', 'storage'
    )),
    -- For a storage failure, coven's reason, which the line names.
    storage_failure TEXT CHECK (storage_failure IN (
        'provider_unavailable', 'access_not_registered', 'access_not_granted',
        'dropbox_app_folder_not_shareable', 'catch_up_required', 'network',
        'authentication', 'permission_denied', 'not_found', 'already_exists',
        'conflict', 'container_not_found', 'region_mismatch', 'store_exists',
        'clock_skew', 'own_bucket_required', 'quota_exceeded', 'rate_limited',
        'invalid_configuration', 'refused', 'protocol', 'invalid_path',
        'invalid_range', 'not_store_owner', 'invitee_is_store_owner',
        'account_id_unavailable', 'invitation_mismatch', 'session_mismatch',
        'session_expired', 'session_identity_unknown', 'invalid_part',
        'single_request_too_large', 'copy_too_large', 'encoding', 'file',
        'member_keys_missing'
    )),
    -- For an S3 location that can't hold a store needing its own bucket:
    -- whether it names a base prefix or the bucket holds other objects.
    own_bucket_reason TEXT CHECK (own_bucket_reason IN ('prefix', 'occupied')),
    -- For a sign-in that lacks access: the access it must have.
    required_access TEXT CHECK (required_access IN (
        'dropbox_app_folder', 'dropbox_full', 'one_drive'
    )),
    -- For a body past what the provider takes in one request or one copy:
    -- its size and that limit, in bytes.
    size_bytes INTEGER CHECK (size_bytes >= 0),
    limit_bytes INTEGER CHECK (limit_bytes >= 0),
    -- The failure's untranslated text, with every cause below it.
    detail TEXT NOT NULL,
    CHECK ((storage_failure IS NOT NULL) = (category = 'storage')),
    CHECK ((own_bucket_reason IS NOT NULL)
        = (storage_failure IS 'own_bucket_required')),
    CHECK ((required_access IS NOT NULL) = coalesce(
        storage_failure IN ('access_not_registered', 'access_not_granted'), 0
    )),
    CHECK ((size_bytes IS NOT NULL) = coalesce(
        storage_failure IN ('single_request_too_large', 'copy_too_large'), 0
    )),
    CHECK ((limit_bytes IS NOT NULL) = (size_bytes IS NOT NULL))
) STRICT;

-- A start that got under way ends the failed start before it, in the
-- transaction that starts it: coven counts every start of sync in its sync
-- status row, a setup's among them, even one whose passes end the way the
-- last one did. A setup starts sync inside coven, so no start of bae's
-- returns to clear the row, and a status that says sync runs never shows
-- beside a start that failed before.
CREATE TRIGGER IF NOT EXISTS sync_start_failure_ended
AFTER UPDATE OF starts ON coven_sync_status
WHEN NEW.starts IS NOT OLD.starts
BEGIN
    DELETE FROM sync_start_failure;
END;
