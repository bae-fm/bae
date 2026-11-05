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

-- How many times the rows of each table the library's paged lists sort have
-- moved: a row inserted or deleted, or one of the values its list sorts or
-- filters it by changed. Triggers on each table advance its version in the
-- write that moves its rows. A list's read starts a window from a row an earlier read
-- found only while the versions of the tables its order reads stand where
-- they stood for that read (§9.2).
CREATE TABLE IF NOT EXISTS library_list_versions (
    source  TEXT PRIMARY KEY CHECK (source IN (
        'album_listing', 'album_artist_link', 'artist_listing', 'composer_listing',
        'composer_detail_row', 'release_listing', 'library_search')),
    version INTEGER NOT NULL
) STRICT;

INSERT OR IGNORE INTO library_list_versions (source, version) VALUES
    ('album_listing', 0), ('album_artist_link', 0), ('artist_listing', 0),
    ('composer_listing', 0), ('composer_detail_row', 0), ('release_listing', 0),
    ('library_search', 0);

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

CREATE TRIGGER IF NOT EXISTS composer_listing_order_added AFTER INSERT ON composer_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_listing';
END;

CREATE TRIGGER IF NOT EXISTS composer_listing_order_removed AFTER DELETE ON composer_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_listing';
END;

CREATE TRIGGER IF NOT EXISTS composer_listing_order_moved AFTER UPDATE OF name, work_count, linked_release_count ON composer_listing
WHEN OLD.name IS NOT NEW.name
  OR OLD.work_count IS NOT NEW.work_count
  OR OLD.linked_release_count IS NOT NEW.linked_release_count
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_listing';
END;

-- Each row added to or removed from the three tables counts in or out of its
-- artist's listing row. An artist whose own row has not arrived yet gets its
-- listing row when it does (`composer_artist_shown_added`).
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
-- `composer_work`. These two views hold no rows: they name the statements
-- their INSTEAD OF triggers run, which the triggers on the library tables
-- share.
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

-- An artist's shown row arriving gives it a listing row for what was
-- already counted under it, then counts its own credits. It fires on the
-- shown row rather than the artist, because the counts read
-- `work_composers`, which joins `artist_shown_as`: the row's insert orders
-- them after it.
CREATE TRIGGER IF NOT EXISTS composer_artist_shown_added AFTER INSERT ON artist_shown_as
BEGIN
    INSERT INTO composer_listing (artist_id, name, work_count, linked_release_count, unlinked_credit_count)
    SELECT artist.id, artist.name,
           (SELECT COUNT(*) FROM composer_work WHERE artist_id = artist.id),
           (SELECT COUNT(*) FROM composer_release WHERE artist_id = artist.id),
           (SELECT COUNT(*) FROM composer_unlinked_credit WHERE artist_id = artist.id)
    FROM artists artist
    WHERE artist.id = NEW.artist_id
      AND (EXISTS (SELECT 1 FROM composer_work WHERE artist_id = artist.id)
        OR EXISTS (SELECT 1 FROM composer_release WHERE artist_id = artist.id)
        OR EXISTS (SELECT 1 FROM composer_unlinked_credit WHERE artist_id = artist.id));
    INSERT INTO composer_work_refresh (work_id)
    SELECT DISTINCT work_id FROM work_artists WHERE artist_id = NEW.artist_id;
    INSERT INTO composer_release_refresh (release_id)
    SELECT DISTINCT release_id FROM artist_credited_releases WHERE artist_id = NEW.artist_id;
END;

-- An artist's shown artist moving rewrites the composer rows of every work
-- and release it is credited on, as `album_credit_shown_changed` rewrites
-- its albums. An artist whose shown artist stayed changes no row.
CREATE TRIGGER IF NOT EXISTS composer_artist_shown_changed AFTER UPDATE OF shown_artist_id ON artist_shown_as
WHEN OLD.shown_artist_id IS NOT NEW.shown_artist_id
BEGIN
    INSERT INTO composer_work_refresh (work_id)
    SELECT DISTINCT work_id FROM work_artists WHERE artist_id = NEW.artist_id;
    INSERT INTO composer_release_refresh (release_id)
    SELECT DISTINCT release_id FROM artist_credited_releases WHERE artist_id = NEW.artist_id;
END;

CREATE TRIGGER IF NOT EXISTS artist_renamed AFTER UPDATE OF name ON artists
BEGIN
    UPDATE composer_listing SET name = NEW.name WHERE artist_id = NEW.id;
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

-- A composer's page (§2.4) lists its works, then its unlinked release
-- credits, then its unlinked track credits, in one order that reads works'
-- titles and parts and the credits' albums, releases and tracks. No index
-- reaches across those, so `composer_detail_row` holds each row of each page
-- with what it sorts by, kept by the triggers below.

-- Each row of each composer's page, in the page's order. In the works, each
-- credited work is listed under every work the library holds that it is part
-- of, each of those heading its group whether the composer is credited on it
-- or not, and a credited work part of none heads a group of its own; a group
-- sorts by its heading work's title, its works heading first, then by title.
-- Each unlinked credit is listed under its release, the releases by album
-- title, then by album, so two albums sharing a title don't interleave, then
-- by the release's place among its album's; the credits by track and then by
-- their own order. Titles compare without case, as the album grid's do.
CREATE TABLE IF NOT EXISTS composer_detail_row (
    -- The row's number: what a screen tells it apart by.
    entry       INTEGER PRIMARY KEY,
    -- The composer, as the composer list shows it.
    artist_id   TEXT NOT NULL,
    -- The part of the page: 0 the works, 1 the release credits, 2 the track
    -- credits.
    section     INTEGER NOT NULL CHECK (section IN (0, 1, 2)),
    -- The row's group: in the works, the work heading it; for a credit, its
    -- release.
    group_id    TEXT NOT NULL,
    -- The heading work's title, or the release's album's.
    group_title TEXT NOT NULL COLLATE NOCASE,
    -- For a credit, the release's album.
    group_album TEXT,
    -- For a credit, the release's place among its album's (`album_order`).
    group_order REAL,
    -- In the works, 0 for the work heading its group and 1 for a part; for a
    -- track credit, its track's position. NULL for a release credit.
    place       INTEGER,
    -- A work's title.
    title       TEXT COLLATE NOCASE,
    -- A credit's position among its release's or its track's.
    position    INTEGER,
    -- The work or the credit the row shows.
    item_id     TEXT NOT NULL,
    UNIQUE (artist_id, section, group_id, item_id),
    CHECK ((section = 0) = (title IS NOT NULL)),
    CHECK ((section = 0) = (group_album IS NULL)),
    CHECK ((section = 0) = (group_order IS NULL)),
    CHECK ((section = 0) = (position IS NULL)),
    CHECK ((section = 1) = (place IS NULL)),
    CHECK (section <> 0 OR place = (group_id <> item_id))
) STRICT;

CREATE INDEX IF NOT EXISTS idx_composer_detail_row_order ON composer_detail_row
    (artist_id, section, group_title, group_album, group_order, group_id, place, title, position, item_id);

CREATE INDEX IF NOT EXISTS idx_composer_detail_row_item ON composer_detail_row (item_id);

CREATE TRIGGER IF NOT EXISTS composer_detail_row_order_added AFTER INSERT ON composer_detail_row
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_detail_row';
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_row_order_removed AFTER DELETE ON composer_detail_row
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_detail_row';
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_row_order_moved AFTER UPDATE OF group_title, group_album, group_order, place, title, position ON composer_detail_row
WHEN OLD.group_title IS NOT NEW.group_title
  OR OLD.group_album IS NOT NEW.group_album
  OR OLD.group_order IS NOT NEW.group_order
  OR OLD.place IS NOT NEW.place
  OR OLD.title IS NOT NEW.title
  OR OLD.position IS NOT NEW.position
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_detail_row';
END;

-- The works rows of every composer's page: each credited work under each
-- work it is part of, each such work heading its group, and each credited
-- work part of none the library holds heading its own. A refresh reads it
-- for one artist and one work, which SQLite pushes into each arm; the
-- heading arm is read from the heading work through its parts (CROSS JOIN
-- keeps that order), so it costs the work's parts rather than every work
-- the artist is credited on.
CREATE VIEW IF NOT EXISTS composer_detail_works AS
SELECT credited.artist_id, part.parent_work_id AS group_id, parent.title AS group_title,
       1 AS place, work.title, credited.work_id AS item_id
FROM composer_work credited
JOIN work_parts part ON part.child_work_id = credited.work_id
JOIN works parent ON parent.id = part.parent_work_id
JOIN works work ON work.id = credited.work_id
UNION
SELECT credited.artist_id, parent.id, parent.title, 0, parent.title, parent.id
FROM works parent
CROSS JOIN work_parts part ON part.parent_work_id = parent.id
CROSS JOIN composer_work credited ON credited.work_id = part.child_work_id
UNION
SELECT credited.artist_id, work.id, work.title, 0, work.title, work.id
FROM composer_work credited
JOIN works work ON work.id = credited.work_id
WHERE NOT EXISTS (
    SELECT 1 FROM work_parts part JOIN works parent ON parent.id = part.parent_work_id
    WHERE part.child_work_id = credited.work_id);

-- The credit rows of every composer's page: each unlinked credit under its
-- release, once its release and album are held.
CREATE VIEW IF NOT EXISTS composer_detail_credits AS
SELECT unlinked.artist_id, 1 AS section, unlinked.release_id AS group_id,
       album.title AS group_title, album.id AS group_album, r.album_order AS group_order,
       NULL AS place, rar.position, unlinked.credit_id AS item_id
FROM composer_unlinked_credit unlinked
JOIN release_artist_roles rar ON rar.id = unlinked.credit_id
JOIN releases r ON r.id = unlinked.release_id
JOIN albums album ON album.id = r.album_id
UNION ALL
SELECT unlinked.artist_id, 2, unlinked.release_id, album.title, album.id, r.album_order,
       t.position, tar.position, unlinked.credit_id
FROM composer_unlinked_credit unlinked
JOIN track_artist_roles tar ON tar.id = unlinked.credit_id
JOIN tracks t ON t.id = tar.track_id
JOIN releases r ON r.id = unlinked.release_id
JOIN albums album ON album.id = r.album_id;

-- Inserting an artist and a work into `composer_detail_work_refresh`
-- rewrites the rows that list that work on that artist's page from
-- `composer_detail_works`, and inserting a credit into
-- `composer_detail_credit_refresh` rewrites the row that lists it from
-- `composer_detail_credits`: each adds and removes only the rows that differ
-- and updates a row in place where only what it sorts by changed. The views
-- hold no rows; they name the statements their INSTEAD OF triggers run.
CREATE VIEW IF NOT EXISTS composer_detail_work_refresh AS SELECT NULL AS artist_id, NULL AS work_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS composer_detail_work_refresh INSTEAD OF INSERT ON composer_detail_work_refresh
BEGIN
    DELETE FROM composer_detail_row
    WHERE artist_id = NEW.artist_id AND section = 0 AND item_id = NEW.work_id
      AND group_id NOT IN (
          SELECT group_id FROM composer_detail_works
          WHERE artist_id = NEW.artist_id AND item_id = NEW.work_id);
    INSERT INTO composer_detail_row (artist_id, section, group_id, group_title, place, title, item_id)
    SELECT artist_id, 0, group_id, group_title, place, title, item_id FROM composer_detail_works
    WHERE artist_id = NEW.artist_id AND item_id = NEW.work_id
    ON CONFLICT (artist_id, section, group_id, item_id) DO UPDATE SET
        group_title = excluded.group_title,
        place = excluded.place,
        title = excluded.title
    WHERE composer_detail_row.group_title IS NOT excluded.group_title
       OR composer_detail_row.place IS NOT excluded.place
       OR composer_detail_row.title IS NOT excluded.title;
END;

CREATE VIEW IF NOT EXISTS composer_detail_credit_refresh AS SELECT NULL AS credit_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS composer_detail_credit_refresh INSTEAD OF INSERT ON composer_detail_credit_refresh
BEGIN
    DELETE FROM composer_detail_row
    WHERE item_id = NEW.credit_id AND section <> 0
      AND (artist_id, section, group_id) NOT IN (
          SELECT artist_id, section, group_id FROM composer_detail_credits
          WHERE item_id = NEW.credit_id);
    INSERT INTO composer_detail_row
        (artist_id, section, group_id, group_title, group_album, group_order, place, position,
         item_id)
    SELECT artist_id, section, group_id, group_title, group_album, group_order, place, position,
           item_id
    FROM composer_detail_credits WHERE item_id = NEW.credit_id
    ON CONFLICT (artist_id, section, group_id, item_id) DO UPDATE SET
        group_title = excluded.group_title,
        group_album = excluded.group_album,
        group_order = excluded.group_order,
        place = excluded.place,
        position = excluded.position
    WHERE composer_detail_row.group_title IS NOT excluded.group_title
       OR composer_detail_row.group_album IS NOT excluded.group_album
       OR composer_detail_row.group_order IS NOT excluded.group_order
       OR composer_detail_row.place IS NOT excluded.place
       OR composer_detail_row.position IS NOT excluded.position;
END;

-- A work credited on an artist or no longer lists that work on the artist's
-- page, and the works it is part of, which head a group while any of their
-- parts is credited there.
CREATE TRIGGER IF NOT EXISTS composer_detail_work_credited AFTER INSERT ON composer_work
BEGIN
    INSERT INTO composer_detail_work_refresh (artist_id, work_id)
    SELECT NEW.artist_id, NEW.work_id
    UNION ALL
    SELECT NEW.artist_id, parent_work_id FROM work_parts WHERE child_work_id = NEW.work_id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_work_uncredited AFTER DELETE ON composer_work
BEGIN
    INSERT INTO composer_detail_work_refresh (artist_id, work_id)
    SELECT OLD.artist_id, OLD.work_id
    UNION ALL
    SELECT OLD.artist_id, parent_work_id FROM work_parts WHERE child_work_id = OLD.work_id;
END;

-- A part joining or leaving a work moves the part on every page it is
-- credited on, and the work's heading there.
CREATE TRIGGER IF NOT EXISTS composer_detail_part_added AFTER INSERT ON work_parts
BEGIN
    INSERT INTO composer_detail_work_refresh (artist_id, work_id)
    SELECT artist_id, NEW.child_work_id FROM composer_work WHERE work_id = NEW.child_work_id
    UNION ALL
    SELECT artist_id, NEW.parent_work_id FROM composer_work WHERE work_id = NEW.child_work_id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_part_changed AFTER UPDATE OF parent_work_id, child_work_id ON work_parts
BEGIN
    INSERT INTO composer_detail_work_refresh (artist_id, work_id)
    SELECT artist_id, OLD.child_work_id FROM composer_work WHERE work_id = OLD.child_work_id
    UNION ALL
    SELECT artist_id, OLD.parent_work_id FROM composer_work WHERE work_id = OLD.child_work_id
    UNION ALL
    SELECT artist_id, NEW.child_work_id FROM composer_work WHERE work_id = NEW.child_work_id
    UNION ALL
    SELECT artist_id, NEW.parent_work_id FROM composer_work WHERE work_id = NEW.child_work_id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_part_removed AFTER DELETE ON work_parts
BEGIN
    INSERT INTO composer_detail_work_refresh (artist_id, work_id)
    SELECT artist_id, OLD.child_work_id FROM composer_work WHERE work_id = OLD.child_work_id
    UNION ALL
    SELECT artist_id, OLD.parent_work_id FROM composer_work WHERE work_id = OLD.child_work_id;
END;

-- A work arriving, retitled or gone changes every row that shows its title:
-- its own on the pages it is credited on or heads a group on, and its parts'
-- on theirs, which it heads, or which headed their own while it was missing.
CREATE VIEW IF NOT EXISTS composer_detail_title_refresh AS SELECT NULL AS work_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS composer_detail_title_refresh INSTEAD OF INSERT ON composer_detail_title_refresh
BEGIN
    INSERT INTO composer_detail_work_refresh (artist_id, work_id)
    SELECT artist_id, NEW.work_id FROM composer_work WHERE work_id = NEW.work_id
    UNION
    SELECT credited.artist_id, NEW.work_id FROM work_parts part
    JOIN composer_work credited ON credited.work_id = part.child_work_id
    WHERE part.parent_work_id = NEW.work_id
    UNION
    SELECT credited.artist_id, credited.work_id FROM work_parts part
    JOIN composer_work credited ON credited.work_id = part.child_work_id
    WHERE part.parent_work_id = NEW.work_id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_work_added AFTER INSERT ON works
BEGIN
    INSERT INTO composer_detail_title_refresh (work_id) VALUES (NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_work_retitled AFTER UPDATE OF title ON works
WHEN OLD.title IS NOT NEW.title
BEGIN
    INSERT INTO composer_detail_title_refresh (work_id) VALUES (NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_work_removed AFTER DELETE ON works
BEGIN
    INSERT INTO composer_detail_title_refresh (work_id) VALUES (OLD.id);
END;

-- An unlinked credit counted in or out lists or unlists it.
CREATE TRIGGER IF NOT EXISTS composer_detail_credit_added AFTER INSERT ON composer_unlinked_credit
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id) VALUES (NEW.credit_id);
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_credit_removed AFTER DELETE ON composer_unlinked_credit
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id) VALUES (OLD.credit_id);
END;

-- What a credit sorts by changing moves it: its album arriving or retitled,
-- its release arriving, moving to another album or to another place among
-- its album's, its track moving, or its own position.
CREATE TRIGGER IF NOT EXISTS composer_detail_album_added AFTER INSERT ON albums
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id)
    SELECT unlinked.credit_id FROM releases r
    JOIN composer_unlinked_credit unlinked ON unlinked.release_id = r.id
    WHERE r.album_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_album_retitled AFTER UPDATE OF title ON albums
WHEN OLD.title IS NOT NEW.title
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id)
    SELECT unlinked.credit_id FROM releases r
    JOIN composer_unlinked_credit unlinked ON unlinked.release_id = r.id
    WHERE r.album_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_release_added AFTER INSERT ON releases
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id)
    SELECT credit_id FROM composer_unlinked_credit WHERE release_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_release_moved AFTER UPDATE OF album_id, album_order ON releases
WHEN OLD.album_id IS NOT NEW.album_id OR OLD.album_order IS NOT NEW.album_order
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id)
    SELECT credit_id FROM composer_unlinked_credit WHERE release_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_track_moved AFTER UPDATE OF position ON tracks
WHEN OLD.position IS NOT NEW.position
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id)
    SELECT unlinked.credit_id FROM track_artist_roles tar
    JOIN composer_unlinked_credit unlinked ON unlinked.credit_id = tar.id
    WHERE tar.track_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_release_role_moved AFTER UPDATE OF position ON release_artist_roles
WHEN OLD.position IS NOT NEW.position
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id) VALUES (NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_track_role_moved AFTER UPDATE OF position ON track_artist_roles
WHEN OLD.position IS NOT NEW.position
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id) VALUES (NEW.id);
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

CREATE TRIGGER IF NOT EXISTS album_listing_order_added AFTER INSERT ON album_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_listing';
END;

CREATE TRIGGER IF NOT EXISTS album_listing_order_removed AFTER DELETE ON album_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_listing';
END;

CREATE TRIGGER IF NOT EXISTS album_listing_order_moved AFTER UPDATE OF title, year, created_at, credit_sort_names, credit_artist_ids ON album_listing
WHEN OLD.title IS NOT NEW.title
  OR OLD.year IS NOT NEW.year
  OR OLD.created_at IS NOT NEW.created_at
  OR OLD.credit_sort_names IS NOT NEW.credit_sort_names
  OR OLD.credit_artist_ids IS NOT NEW.credit_artist_ids
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_listing';
END;

-- Each album the library shows and each artist one of its credits shows,
-- once however many credits name that artist.
CREATE TABLE IF NOT EXISTS album_artist_link (
    artist_id TEXT NOT NULL,
    album_id TEXT NOT NULL REFERENCES albums (id) ON DELETE CASCADE,
    PRIMARY KEY (artist_id, album_id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_album_artist_link_album ON album_artist_link (album_id);

CREATE TRIGGER IF NOT EXISTS album_artist_link_order_added AFTER INSERT ON album_artist_link
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_artist_link';
END;

CREATE TRIGGER IF NOT EXISTS album_artist_link_order_removed AFTER DELETE ON album_artist_link
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_artist_link';
END;

CREATE TRIGGER IF NOT EXISTS album_artist_link_order_moved AFTER UPDATE OF artist_id, album_id ON album_artist_link
WHEN OLD.artist_id IS NOT NEW.artist_id
  OR OLD.album_id IS NOT NEW.album_id
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_artist_link';
END;

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

CREATE TRIGGER IF NOT EXISTS artist_listing_order_added AFTER INSERT ON artist_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_listing';
END;

CREATE TRIGGER IF NOT EXISTS artist_listing_order_removed AFTER DELETE ON artist_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_listing';
END;

CREATE TRIGGER IF NOT EXISTS artist_listing_order_moved AFTER UPDATE OF name, sort_key, album_count ON artist_listing
WHEN OLD.name IS NOT NEW.name
  OR OLD.sort_key IS NOT NEW.sort_key
  OR OLD.album_count IS NOT NEW.album_count
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_listing';
END;

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
-- credits, writing the row only where a value differs and adding and removing
-- only the links that differ: a release added to an album of many changes
-- none of its other releases' listing rows.
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
        credit_artist_ids = excluded.credit_artist_ids
    WHERE album_listing.title IS NOT excluded.title
       OR album_listing.year IS NOT excluded.year
       OR album_listing.created_at IS NOT excluded.created_at
       OR album_listing.credit_sort_names IS NOT excluded.credit_sort_names
       OR album_listing.credit_artist_ids IS NOT excluded.credit_artist_ids;
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

-- A release arriving or moving lists its albums through `release_added` and
-- `release_changed`, below, which list the release in the same step.

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
-- credits, its media, its files' count and size, whether they are stored and
-- whether this device keeps them all pinned, how many of its files wait in
-- this device's upload queue, and when it was added, sorted by them and
-- filtered by whether they are stored. `release_listing` holds those values, kept by the triggers
-- below, with an index for each sort, alone and after `stored` for the Remote
-- and Local filters.

-- Each release, with what the Storage Manager sorts and filters it by:
-- `album_title` and the credit keys copied from its album's `album_listing`
-- row; `media_empty` and `media_key` as `release_media_sort` works them out
-- from its `release_media`; `created_at`, when the release was added; its
-- files' count and total size, `stored`, `pinned_count` and `queued_count`,
-- copied from its `release_storage` row; `download_queued`, 1 while this
-- device's download queue holds it waiting or downloading; and
-- `storage_order`, where it sorts by storage, worked out from those in the
-- same write: uploading (0), pinning (1) while the download queue brings its
-- files to this device, local (2), pinned (3), in the cloud only (4).
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
    stored INTEGER NOT NULL,
    pinned_count INTEGER NOT NULL,
    queued_count INTEGER NOT NULL,
    download_queued INTEGER NOT NULL CHECK (download_queued IN (0, 1)),
    storage_order INTEGER GENERATED ALWAYS AS (CASE
        WHEN queued_count > 0 THEN 0
        WHEN download_queued = 1 THEN 1
        WHEN stored = 0 THEN 2
        WHEN file_count > 0 AND pinned_count = file_count THEN 3
        ELSE 4 END) VIRTUAL
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
CREATE INDEX IF NOT EXISTS idx_release_listing_storage ON release_listing (storage_order, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_storage_desc ON release_listing (storage_order DESC, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_added ON release_listing (created_at, album_title COLLATE NOCASE, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_added_desc ON release_listing (created_at DESC, album_title COLLATE NOCASE, release_id);

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
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_storage ON release_listing (stored, storage_order, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_storage_desc ON release_listing (stored, storage_order DESC, album_title COLLATE NOCASE, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_added ON release_listing (stored, created_at, album_title COLLATE NOCASE, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_added_desc ON release_listing (stored, created_at DESC, album_title COLLATE NOCASE, release_id);

CREATE TRIGGER IF NOT EXISTS release_listing_order_added AFTER INSERT ON release_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'release_listing';
END;

CREATE TRIGGER IF NOT EXISTS release_listing_order_removed AFTER DELETE ON release_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'release_listing';
END;

CREATE TRIGGER IF NOT EXISTS release_listing_order_moved AFTER UPDATE OF album_title, credit_sort_names, credit_artist_ids, media_empty, media_key, created_at, file_count, total_size, stored, queued_count, download_queued, pinned_count ON release_listing
WHEN OLD.album_title IS NOT NEW.album_title
  OR OLD.credit_sort_names IS NOT NEW.credit_sort_names
  OR OLD.credit_artist_ids IS NOT NEW.credit_artist_ids
  OR OLD.media_empty IS NOT NEW.media_empty
  OR OLD.media_key IS NOT NEW.media_key
  OR OLD.created_at IS NOT NEW.created_at
  OR OLD.file_count IS NOT NEW.file_count
  OR OLD.total_size IS NOT NEW.total_size
  OR OLD.stored IS NOT NEW.stored
  OR OLD.storage_order IS NOT NEW.storage_order
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'release_listing';
END;

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

-- Each release's media as the Storage Manager sorts by them: `media_empty`, 1
-- for a release stating no medium, and `media_key`, every stated medium with
-- its count, in order.
CREATE VIEW IF NOT EXISTS release_media_sort AS
SELECT r.id AS release_id,
       NOT EXISTS (SELECT 1 FROM release_media m WHERE m.release_id = r.id) AS media_empty,
       (SELECT group_concat(printf('%s:%05d', m.medium, m.count), ',' ORDER BY m.position)
        FROM release_media m WHERE m.release_id = r.id) AS media_key
FROM releases r;

-- Inserting a release id into `release_listing_refresh` rewrites that
-- release's row of `release_listing` from the release, its storage row and
-- its album's `album_listing` row; a release whose album is not listed yet
-- gets its row when the album's is written, one whose storage row is not
-- written yet when that is.
CREATE VIEW IF NOT EXISTS release_listing_refresh AS SELECT NULL AS release_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS release_listing_refresh INSTEAD OF INSERT ON release_listing_refresh
BEGIN
    INSERT INTO release_listing (
        release_id, album_id, album_title, credit_sort_names, credit_artist_ids,
        media_empty, media_key, created_at, file_count, total_size, stored, pinned_count,
        queued_count, download_queued)
    SELECT r.id, r.album_id, album.title, album.credit_sort_names, album.credit_artist_ids,
           media.media_empty,
           media.media_key,
           r.created_at,
           storage.file_count,
           storage.total_size,
           storage.stored,
           storage.pinned_count,
           storage.queued_count,
           EXISTS (SELECT 1 FROM download_queue download
                   WHERE download.release_id = r.id AND download.state IN ('queued', 'active'))
    FROM releases r
    JOIN release_storage storage ON storage.release_id = r.id
    JOIN album_listing album ON album.album_id = r.album_id
    JOIN release_media_sort media ON media.release_id = r.id
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
        stored = excluded.stored,
        pinned_count = excluded.pinned_count,
        queued_count = excluded.queued_count,
        download_queued = excluded.download_queued;
END;

-- A release arriving lists its album first. An album listed just now lists
-- every release it holds, this one with them (`release_listing_album_listed`);
-- a release joining an album already listed is refreshed by itself. Either
-- refresh waits for the release's storage row, which
-- `release_storage_release_added` writes as the release arrives, before or
-- after this trigger runs; when it runs after, the row's arrival refreshes
-- the release (`release_listing_storage_added`). So its row is refreshed once
-- whichever runs first. The two steps here share one trigger because their
-- order decides that, and SQLite names no order for several triggers on one
-- event.
CREATE TRIGGER IF NOT EXISTS release_added AFTER INSERT ON releases
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.album_id);
    INSERT INTO release_listing_refresh (release_id)
    SELECT NEW.id WHERE NOT EXISTS (SELECT 1 FROM release_listing WHERE release_id = NEW.id)
        AND EXISTS (SELECT 1 FROM release_storage WHERE release_id = NEW.id);
END;

-- A release moving to another album lists the album it left and the one it
-- joined first. An album listed just now lists every release it holds, this
-- one with them, so its row then names that album; otherwise, and for a change
-- to what the release states itself, the release is refreshed by itself. Its
-- row is refreshed once either way, in one trigger for the same reason as
-- `release_added`.
CREATE TRIGGER IF NOT EXISTS release_changed AFTER UPDATE OF album_id, created_at ON releases
BEGIN
    INSERT INTO album_listing_refresh (album_id)
    SELECT OLD.album_id WHERE OLD.album_id IS NOT NEW.album_id
    UNION ALL
    SELECT NEW.album_id WHERE OLD.album_id IS NOT NEW.album_id;
    INSERT INTO release_listing_refresh (release_id)
    SELECT NEW.id WHERE OLD.album_id IS NEW.album_id
        OR NOT EXISTS (SELECT 1 FROM release_listing
                       WHERE release_id = NEW.id AND album_id = NEW.album_id);
END;

-- A release's media changing rewrites only its listing row's media: a
-- release arriving with its media is refreshed once by `release_added`, and
-- its media rows, written after it, set the two columns that row's refresh
-- found none for. A release no row lists yet moves nothing.
CREATE TRIGGER IF NOT EXISTS release_listing_media_added AFTER INSERT ON release_media
BEGIN
    UPDATE release_listing SET (media_empty, media_key) =
        (SELECT media_empty, media_key FROM release_media_sort
         WHERE release_id = NEW.release_id)
    WHERE release_id = NEW.release_id;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_media_changed AFTER UPDATE OF release_id, position, medium, count ON release_media
BEGIN
    UPDATE release_listing SET (media_empty, media_key) =
        (SELECT media_empty, media_key FROM release_media_sort media
         WHERE media.release_id = release_listing.release_id)
    WHERE release_id IN (OLD.release_id, NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS release_listing_media_removed AFTER DELETE ON release_media
BEGIN
    UPDATE release_listing SET (media_empty, media_key) =
        (SELECT media_empty, media_key FROM release_media_sort
         WHERE release_id = OLD.release_id)
    WHERE release_id = OLD.release_id;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_album_listed AFTER INSERT ON album_listing
BEGIN
    INSERT INTO release_listing_refresh (release_id)
    SELECT r.id FROM releases r
    WHERE r.album_id = NEW.album_id
      AND EXISTS (SELECT 1 FROM release_storage storage WHERE storage.release_id = r.id);
END;

CREATE TRIGGER IF NOT EXISTS release_listing_album_changed AFTER UPDATE OF title, credit_sort_names, credit_artist_ids ON album_listing
WHEN OLD.title IS NOT NEW.title
  OR OLD.credit_sort_names IS NOT NEW.credit_sort_names
  OR OLD.credit_artist_ids IS NOT NEW.credit_artist_ids
BEGIN
    INSERT INTO release_listing_refresh (release_id)
    SELECT id FROM releases WHERE album_id = NEW.album_id;
END;

-- A release's storage row arriving lists the release once its album is
-- listed, and its counts changing copy into the release's listing row,
-- which sorts and filters by them. With `release_added` and
-- `release_listing_album_listed`, each refreshing a release only once the
-- other of its storage row and its album's listing row is there, a release
-- arriving is refreshed once, whichever of the triggers on `releases` runs
-- first.
CREATE TRIGGER IF NOT EXISTS release_listing_storage_added AFTER INSERT ON release_storage
BEGIN
    INSERT INTO release_listing_refresh (release_id)
    SELECT NEW.release_id WHERE EXISTS (
        SELECT 1 FROM releases r JOIN album_listing album ON album.album_id = r.album_id
        WHERE r.id = NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS release_listing_storage_changed AFTER UPDATE OF file_count, total_size, stored, pinned_count, queued_count ON release_storage
BEGIN
    UPDATE release_listing SET file_count = NEW.file_count, total_size = NEW.total_size,
        stored = NEW.stored, pinned_count = NEW.pinned_count, queued_count = NEW.queued_count
    WHERE release_id = NEW.release_id;
END;

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

-- What library search finds of albums (above) moves with an album's title,
-- an artist's name, an artist's other printed names, an album's credits and
-- the artist each credit shows as: every write changing one of them advances
-- `library_search` in `library_list_versions`, so a list narrowed to what it
-- finds, the Storage Manager's (§4.2), knows when its rows may have moved.
CREATE TRIGGER IF NOT EXISTS library_search_album_added AFTER INSERT ON albums
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_album_changed AFTER UPDATE OF id, title ON albums
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_album_removed AFTER DELETE ON albums
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_added AFTER INSERT ON artists
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_changed AFTER UPDATE OF id, name ON artists
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_removed AFTER DELETE ON artists
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_credit_added AFTER INSERT ON artist_credits
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_credit_changed AFTER UPDATE OF artist_id, name_key, name ON artist_credits
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_credit_removed AFTER DELETE ON artist_credits
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_album_credit_added AFTER INSERT ON album_artists
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_album_credit_changed AFTER UPDATE OF album_id, artist_id, credit_key ON album_artists
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_album_credit_removed AFTER DELETE ON album_artists
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_shown_added AFTER INSERT ON artist_shown_as
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_shown_changed AFTER UPDATE OF artist_id, shown_artist_id ON artist_shown_as
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_shown_removed AFTER DELETE ON artist_shown_as
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

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

-- ── Transfer queues ───────────────────────────────────────────────────────────

-- How many times each transfer queue's rows have moved: an entry added,
-- removed or moved to another place. Triggers on each queue advance its
-- version in the write that moves its entries. A read of a queue starts a
-- window from an entry an earlier read found only while its version stands
-- where it stood for that read (§9.2).
CREATE TABLE IF NOT EXISTS release_queue_versions (
    source  TEXT PRIMARY KEY CHECK (source IN ('download_queue', 'output_queue')),
    version INTEGER NOT NULL
) STRICT;

INSERT OR IGNORE INTO release_queue_versions (source, version) VALUES
    ('download_queue', 0), ('output_queue', 0);

-- The queues this device works through one release at a time: downloads to
-- keep releases offline, and exports and saves. Whether each is paused. Never
-- synced. No transfer outlives the process that ran it, so as the library
-- opens both queues empty and run.
CREATE TABLE IF NOT EXISTS release_queues (
    queue  TEXT PRIMARY KEY CHECK (queue IN ('download', 'output')),
    paused INTEGER NOT NULL CHECK (paused IN (0, 1))
) STRICT;
INSERT OR IGNORE INTO release_queues (queue, paused) VALUES ('download', 0), ('output', 0);

-- A release queued to download for offline, in the order queued. It names its
-- release by id alone: an entry whose release leaves the library stays listed,
-- for it to be cancelled. One entry at most downloads; how far it has got is
-- not here, since it moves with every byte.
CREATE TABLE IF NOT EXISTS download_queue (
    position   INTEGER PRIMARY KEY,
    release_id TEXT NOT NULL UNIQUE,
    -- When it was queued, in milliseconds since the epoch.
    created_at INTEGER NOT NULL,
    state      TEXT NOT NULL CHECK (state IN ('queued', 'active', 'failed')),
    error      TEXT,
    CHECK ((state = 'failed') = (error IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS download_queue_active
    ON download_queue (state) WHERE state = 'active';

CREATE TRIGGER IF NOT EXISTS download_queue_order_added AFTER INSERT ON download_queue
BEGIN
    UPDATE release_queue_versions SET version = version + 1 WHERE source = 'download_queue';
END;

CREATE TRIGGER IF NOT EXISTS download_queue_order_removed AFTER DELETE ON download_queue
BEGIN
    UPDATE release_queue_versions SET version = version + 1 WHERE source = 'download_queue';
END;

CREATE TRIGGER IF NOT EXISTS download_queue_order_moved AFTER UPDATE OF position ON download_queue
WHEN OLD.position IS NOT NEW.position
BEGIN
    UPDATE release_queue_versions SET version = version + 1 WHERE source = 'download_queue';
END;

-- A release joining, moving through or leaving this device's download queue
-- moves its listing's `download_queued` flag, in the transaction that
-- changes the queue: waiting or downloading sets it, a failed download or
-- leaving the queue clears it. An entry names its release by id alone, so one whose
-- release is gone, or not listed yet, moves nothing: the listing's refresh
-- reads the queue as it writes the release's row.
CREATE TRIGGER IF NOT EXISTS release_listing_pin_queued AFTER INSERT ON download_queue
BEGIN
    UPDATE release_listing SET download_queued = NEW.state IN ('queued', 'active')
    WHERE release_id = NEW.release_id;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_pin_changed AFTER UPDATE OF release_id, state ON download_queue
BEGIN
    UPDATE release_listing SET download_queued = 0 WHERE release_id = OLD.release_id;
    UPDATE release_listing SET download_queued = NEW.state IN ('queued', 'active')
    WHERE release_id = NEW.release_id;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_pin_dequeued AFTER DELETE ON download_queue
BEGIN
    UPDATE release_listing SET download_queued = 0 WHERE release_id = OLD.release_id;
END;

-- A release queued to export or save into a folder, as `download_queue` holds
-- a download. How far it has got is not here either.
CREATE TABLE IF NOT EXISTS output_queue (
    position   INTEGER PRIMARY KEY,
    release_id TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    state      TEXT NOT NULL CHECK (state IN ('queued', 'active', 'failed')),
    error      TEXT,
    -- coven's bytes for the folder's path (`coven::path_bytes`).
    target_dir BLOB NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('export', 'save')),
    -- A save's preset, captured whole as JSON when it was queued, so a later
    -- edit to the preset changes nothing already queued.
    preset     TEXT,
    CHECK ((state = 'failed') = (error IS NOT NULL)),
    CHECK ((kind = 'save') = (preset IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS output_queue_active
    ON output_queue (state) WHERE state = 'active';

CREATE TRIGGER IF NOT EXISTS output_queue_order_added AFTER INSERT ON output_queue
BEGIN
    UPDATE release_queue_versions SET version = version + 1 WHERE source = 'output_queue';
END;

CREATE TRIGGER IF NOT EXISTS output_queue_order_removed AFTER DELETE ON output_queue
BEGIN
    UPDATE release_queue_versions SET version = version + 1 WHERE source = 'output_queue';
END;

CREATE TRIGGER IF NOT EXISTS output_queue_order_moved AFTER UPDATE OF position ON output_queue
WHEN OLD.position IS NOT NEW.position
BEGIN
    UPDATE release_queue_versions SET version = version + 1 WHERE source = 'output_queue';
END;

-- A storage transition this process runs for a release while its files rest
-- where they are: a move to the cloud being admitted, until coven's upload
-- queue holds its files; an unpin; or making the release local. Written in
-- the write that admits it, which a release already in one refuses, and
-- deleted as it ends: in the write that finishes its work (an admitted move's
-- that queues its files, an unpin's that releases its pins, a make-local's
-- that switches its files), or in a write of its own after a failure.
-- A pin in flight is the download queue's active entry, not a row here. No
-- transition outlives the process that ran it, so the table is emptied as
-- the library opens.
CREATE TABLE IF NOT EXISTS release_storage_action (
    release_id TEXT PRIMARY KEY REFERENCES releases (id) ON DELETE CASCADE,
    action     TEXT NOT NULL CHECK (action IN ('upload', 'unpin', 'detach_audio'))
) STRICT;

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

-- ── Preferences ───────────────────────────────────────────────────────────────

-- This device's settings for the library: one row, written by each setting's
-- setter, read by what the setting decides and by the settings screens' live
-- query. Never synced: each reflects this device, its link, its network and
-- the servers it runs. A copy of the library new to this device starts at
-- the defaults below.
CREATE TABLE IF NOT EXISTS preferences (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    -- The stored Discogs key's state; NULL while no key is stored. A state
    -- stands for a key in the keyring, so settings show it without reading
    -- the keyring.
    discogs_key TEXT CHECK (discogs_key IN ('unvalidated', 'valid', 'rejected')),
    -- Which loudness a track plays at (§7.5).
    replay_gain_mode TEXT NOT NULL CHECK (replay_gain_mode IN ('off', 'track', 'album')),
    -- The presets a track save and a release save start with. Each applies
    -- to its level, which the setters check; a preset a default names is
    -- replaced or removed only in a write that leaves the default naming one.
    default_track_save_preset   TEXT NOT NULL
        REFERENCES save_preset (id) DEFERRABLE INITIALLY DEFERRED,
    default_release_save_preset TEXT NOT NULL
        REFERENCES save_preset (id) DEFERRABLE INITIALLY DEFERRED,
    -- Whether playback pauses between sides and discs, and how many seconds
    -- such a pause waits before the next side starts on its own; NULL waits
    -- for Play. The countdown is kept while pausing is off.
    pause_between_sides          INTEGER NOT NULL CHECK (pause_between_sides IN (0, 1)),
    side_pause_countdown_seconds INTEGER CHECK (side_pause_countdown_seconds IN (5, 15, 30, 45, 60)),
    -- How many files this device uploads, and downloads, at once.
    max_concurrent_uploads   INTEGER NOT NULL CHECK (max_concurrent_uploads BETWEEN 1 AND 8),
    max_concurrent_downloads INTEGER NOT NULL CHECK (max_concurrent_downloads BETWEEN 1 AND 8),
    show_remaining_time INTEGER NOT NULL CHECK (show_remaining_time IN (0, 1)),
    library_full_width  INTEGER NOT NULL CHECK (library_full_width IN (0, 1)),
    -- Where an import goes: to the cloud home when the library has one, and
    -- whether it stays downloaded here.
    import_to_cloud INTEGER NOT NULL CHECK (import_to_cloud IN (0, 1)),
    import_pinned   INTEGER NOT NULL CHECK (import_pinned IN (0, 1)),
    -- Whether a found release is identified on its own, whether what a run
    -- picks unattended is imported, and which catalogs identification asks.
    identify_automatically INTEGER NOT NULL CHECK (identify_automatically IN (0, 1)),
    import_when_identified INTEGER NOT NULL CHECK (import_when_identified IN (0, 1)),
    ask_musicbrainz        INTEGER NOT NULL CHECK (ask_musicbrainz IN (0, 1)),
    ask_discogs            INTEGER NOT NULL CHECK (ask_discogs IN (0, 1)),
    cast_enabled          INTEGER NOT NULL CHECK (cast_enabled IN (0, 1)),
    cast_transcode_format TEXT NOT NULL CHECK (cast_transcode_format IN ('mp3', 'wav')),
    -- The MCP server; its token is in the keyring.
    mcp_enabled INTEGER NOT NULL CHECK (mcp_enabled IN (0, 1)),
    mcp_port    INTEGER NOT NULL CHECK (mcp_port BETWEEN 1 AND 65535),
    -- The Subsonic server; its password is in the keyring. The address it
    -- binds is an IP address, which the setter checks.
    subsonic_enabled      INTEGER NOT NULL CHECK (subsonic_enabled IN (0, 1)),
    subsonic_port         INTEGER NOT NULL CHECK (subsonic_port BETWEEN 1 AND 65535),
    subsonic_username     TEXT NOT NULL,
    subsonic_bind_address TEXT NOT NULL,
    CHECK (NOT subsonic_enabled OR subsonic_username <> '')
) STRICT;

-- The save presets, in the order the settings list them (§4.6). The editor
-- replaces the list whole.
CREATE TABLE IF NOT EXISTS save_preset (
    id       TEXT PRIMARY KEY CHECK (trim(id) <> ''),
    position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
    name     TEXT NOT NULL CHECK (trim(name) <> ''),
    -- A lossless codec writes at a bit depth, a lossy one at a bitrate.
    codec        TEXT NOT NULL CHECK (codec IN ('flac', 'mp3', 'aac', 'opus_ogg', 'wav', 'aiff')),
    bit_depth    INTEGER CHECK (bit_depth IN (16, 24, 32)),
    bitrate_kbps INTEGER,
    pregap_placement TEXT NOT NULL CHECK (pregap_placement IN (
        'append_to_previous_except_htoa', 'append_to_previous_including_htoa', 'exclude',
        'single_file_with_cue')),
    applies_to_track   INTEGER NOT NULL CHECK (applies_to_track IN (0, 1)),
    applies_to_release INTEGER NOT NULL CHECK (applies_to_release IN (0, 1)),
    embed_cover        INTEGER NOT NULL CHECK (embed_cover IN (0, 1)),
    CHECK (applies_to_track OR applies_to_release),
    CHECK ((codec IN ('flac', 'wav', 'aiff')) = (bit_depth IS NOT NULL)),
    CHECK ((codec IN ('flac', 'wav', 'aiff')) = (bitrate_kbps IS NULL)),
    CHECK (codec <> 'mp3' OR bitrate_kbps BETWEEN 32 AND 320),
    CHECK (codec NOT IN ('aac', 'opus_ogg') OR bitrate_kbps BETWEEN 32 AND 512),
    -- One file with a CUE sheet is a release's, in a codec a sheet can index.
    CHECK (pregap_placement <> 'single_file_with_cue'
        OR (NOT applies_to_track AND codec NOT IN ('aac', 'opus_ogg')))
) STRICT;

-- A save preset's filename pattern: its tokens in order.
CREATE TABLE IF NOT EXISTS save_preset_filename_token (
    preset_id TEXT NOT NULL REFERENCES save_preset (id) ON DELETE CASCADE,
    position  INTEGER NOT NULL CHECK (position >= 0),
    token     TEXT NOT NULL CHECK (token IN (
        'title', 'artist', 'album', 'year', 'track_number', 'disc_number', 'track_total')),
    PRIMARY KEY (preset_id, position)
) STRICT;

INSERT OR IGNORE INTO save_preset (
    id, position, name, codec, bit_depth, bitrate_kbps, pregap_placement,
    applies_to_track, applies_to_release, embed_cover)
VALUES
    ('flac', 0, 'FLAC', 'flac', 24, NULL, 'append_to_previous_except_htoa', 1, 1, 1),
    ('mp3', 1, 'MP3', 'mp3', NULL, 320, 'append_to_previous_except_htoa', 1, 1, 1);
INSERT OR IGNORE INTO save_preset_filename_token (preset_id, position, token)
VALUES ('flac', 0, 'track_number'), ('flac', 1, 'title'),
    ('mp3', 0, 'track_number'), ('mp3', 1, 'title');
INSERT OR IGNORE INTO preferences (
    singleton, discogs_key, replay_gain_mode, default_track_save_preset,
    default_release_save_preset, pause_between_sides, side_pause_countdown_seconds,
    max_concurrent_uploads, max_concurrent_downloads, show_remaining_time,
    library_full_width, import_to_cloud, import_pinned, identify_automatically,
    import_when_identified, ask_musicbrainz, ask_discogs, cast_enabled,
    cast_transcode_format, mcp_enabled, mcp_port, subsonic_enabled, subsonic_port,
    subsonic_username, subsonic_bind_address)
VALUES (
    1, NULL, 'off', 'flac', 'flac', 1, NULL, 3, 3, 0, 0, 1, 1, 1, 0, 1, 1, 0, 'mp3', 0,
    47777, 0, 4533, '', '127.0.0.1');

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
