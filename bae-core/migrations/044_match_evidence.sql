-- A stored match states every barcode its source printed, what its record
-- said the pressing is made of, and the releases its own document named as
-- the same release on other catalogs. The one barcode column held at most one
-- code and nothing said which media a format string described, so the table
-- is rebuilt with a media kind and three ordinal-ordered child tables.
CREATE TABLE import_candidate_match_with_media (
    content_hash           TEXT NOT NULL,
    position               INTEGER NOT NULL CHECK (position >= 0),
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

-- A row with no format described no media. A row with one described it: a
-- Discogs search result's format names and qualifiers, joined with ", ", or
-- the one medium of a MusicBrainz result that a disc ID matched — which is a
-- description, not the release's complete list of media.
INSERT INTO import_candidate_match_with_media (
    content_hash, position, source, release_id, title, artist, year, format, label,
    catalog_number, country, media_kind, cover_url, cover_thumbnail_url, cover_label,
    cover_source, source_group_id, source_tracks_kind, source_tracks_count,
    source_tracks_total_ms, by_disc_id, by_barcode, by_catalog, narrowed_out
)
SELECT content_hash, position, source, release_id, title, artist, year, format, label,
       catalog_number, country,
       CASE WHEN format IS NULL THEN 'undescribed' ELSE 'descriptors' END,
       cover_url, cover_thumbnail_url, cover_label,
       cover_source, source_group_id, source_tracks_kind, source_tracks_count,
       source_tracks_total_ms, by_disc_id, by_barcode, by_catalog, narrowed_out
FROM import_candidate_match;

-- Every barcode the source stated for the match, in the source's order.
CREATE TABLE import_candidate_match_barcode (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    barcode      TEXT NOT NULL CHECK (barcode <> ''),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match_with_media (content_hash, position) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_match_barcode (content_hash, position, ordinal, barcode)
SELECT content_hash, position, 0, barcode
FROM import_candidate_match
WHERE barcode IS NOT NULL;

-- The media a match's record described, in the record's order. A row's
-- media kind is its match's: a format is absent only for a medium the record
-- listed without stating one, and an undescribed match has no rows.
CREATE TABLE import_candidate_match_medium (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    media_kind   TEXT NOT NULL CHECK (media_kind IN ('per_medium', 'descriptors')),
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    format       TEXT CHECK (format IS NULL OR format <> ''),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position, media_kind)
        REFERENCES import_candidate_match_with_media (content_hash, position, media_kind)
        ON DELETE CASCADE,
    CHECK (media_kind = 'per_medium' OR format IS NOT NULL)
) STRICT;

-- The releases on other catalogs the match's own document named as the same
-- release. No stored match's document was read for these, so there are none
-- to carry across.
CREATE TABLE import_candidate_match_link (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    catalog      TEXT NOT NULL CHECK (catalog <> ''),
    key          TEXT NOT NULL CHECK (key <> ''),
    PRIMARY KEY (content_hash, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match_with_media (content_hash, position) ON DELETE CASCADE
) STRICT;

DROP TABLE import_candidate_match;
ALTER TABLE import_candidate_match_with_media RENAME TO import_candidate_match;
