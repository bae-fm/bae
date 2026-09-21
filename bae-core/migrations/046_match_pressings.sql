-- A stored match names the pressing row its own run put it in. Re-forming a
-- row from a stored list is not the same answer: a record the run settled as
-- ambiguous because of a record in the other list rolls up when the list is
-- grouped without it, so a reader that re-groups shows rows the run never
-- offered. The table is rebuilt around the column that records the row, and
-- every existing match takes the row the current grouping gives it — the one
-- place re-forming is still right, since nothing else records what the old
-- run built. `match_pressing` is the temp table holding that answer.
CREATE TABLE import_candidate_match_with_pressing (
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

INSERT INTO import_candidate_match_with_pressing (
    content_hash, position, pressing, source, release_id, title, artist, year, format, label,
    catalog_number, country, media_kind, cover_url, cover_thumbnail_url, cover_label,
    cover_source, source_group_id, source_tracks_kind, source_tracks_count,
    source_tracks_total_ms, by_disc_id, by_barcode, by_catalog, narrowed_out
)
SELECT m.content_hash, m.position, row.pressing, m.source, m.release_id, m.title, m.artist,
       m.year, m.format, m.label, m.catalog_number, m.country, m.media_kind, m.cover_url,
       m.cover_thumbnail_url, m.cover_label, m.cover_source, m.source_group_id,
       m.source_tracks_kind, m.source_tracks_count, m.source_tracks_total_ms, m.by_disc_id,
       m.by_barcode, m.by_catalog, m.narrowed_out
FROM import_candidate_match AS m
JOIN match_pressing AS row
  ON row.content_hash = m.content_hash AND row.position = m.position;

-- The child tables reference the match, so they are rebuilt around the table
-- that replaces it.
CREATE TABLE import_candidate_match_barcode_with_pressing (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    barcode      TEXT NOT NULL CHECK (barcode <> ''),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match_with_pressing (content_hash, position) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_match_barcode_with_pressing
SELECT * FROM import_candidate_match_barcode;

CREATE TABLE import_candidate_match_medium_with_pressing (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    media_kind   TEXT NOT NULL CHECK (media_kind IN ('per_medium', 'descriptors')),
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    format       TEXT CHECK (format IS NULL OR format <> ''),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position, media_kind)
        REFERENCES import_candidate_match_with_pressing (content_hash, position, media_kind)
        ON DELETE CASCADE,
    CHECK (media_kind = 'per_medium' OR format IS NOT NULL)
) STRICT;

INSERT INTO import_candidate_match_medium_with_pressing
SELECT * FROM import_candidate_match_medium;

CREATE TABLE import_candidate_match_link_with_pressing (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    catalog      TEXT NOT NULL CHECK (catalog <> ''),
    key          TEXT NOT NULL CHECK (key <> ''),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match_with_pressing (content_hash, position) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_match_link_with_pressing
SELECT * FROM import_candidate_match_link;

DROP TABLE import_candidate_match_barcode;
DROP TABLE import_candidate_match_medium;
DROP TABLE import_candidate_match_link;
DROP TABLE import_candidate_match;

ALTER TABLE import_candidate_match_with_pressing RENAME TO import_candidate_match;
ALTER TABLE import_candidate_match_barcode_with_pressing RENAME TO import_candidate_match_barcode;
ALTER TABLE import_candidate_match_medium_with_pressing RENAME TO import_candidate_match_medium;
ALTER TABLE import_candidate_match_link_with_pressing RENAME TO import_candidate_match_link;

DROP TABLE match_pressing;
