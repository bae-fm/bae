-- The candidate's own text, kept whole, and the matched barcode dropped.
--
-- Extraction used to keep only what its classifier made of the folder's text:
-- at most thirty cluster representatives, as bare strings, with the source, the
-- file and the region thrown away along with every other line. That pool exists
-- to fill the search form's suggestions and is kept for it.
--
-- Ranking asks a different question of the same text — does this result's own
-- catalog number, label, year or country appear in what the folder says? — and
-- a shortlist of representatives cannot answer it. So every line the pass read
-- is stored here, in the order it was read, each naming where it came from.
-- Nothing is extracted from these lines to look anything up.
CREATE TABLE import_candidate_text_line (
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

-- `matched_barcode` named which of the candidate's barcodes the lookup that
-- matched ran against, for a pane that rebuilt what the run had shown. The run
-- now records its own ledger, which states every code every provider walked and
-- what each of them answered, so nothing reads this column any more. A CHECK
-- names it, so the verdict table is rebuilt rather than altered — and its match
-- rows are rebuilt with it, because dropping a parent cascades its children
-- away.
CREATE TABLE import_candidate_verdict_v2 (
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

INSERT INTO import_candidate_verdict_v2 (
    content_hash, kind, track_count, failures_json, ledger_json,
    probed_total_duration_ms, identified_at
)
SELECT content_hash, kind, track_count, failures_json, ledger_json,
       probed_total_duration_ms, identified_at
FROM import_candidate_verdict;

CREATE TABLE import_candidate_match_v2 (
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
    barcode                TEXT,
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
    -- it is not here: that is read out of the text lines above every time the
    -- verdict is read, so changing what the text is taken to state re-ranks the
    -- rows without re-running anything.
    by_disc_id             INTEGER NOT NULL CHECK (by_disc_id IN (0, 1)),
    by_barcode             INTEGER NOT NULL CHECK (by_barcode IN (0, 1)),
    by_catalog             INTEGER NOT NULL CHECK (by_catalog IN (0, 1)),
    narrowed_out           INTEGER NOT NULL DEFAULT 0 CHECK (narrowed_out IN (0, 1)),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_verdict_v2 (content_hash) ON DELETE CASCADE,
    CHECK ((cover_url IS NULL) = (cover_thumbnail_url IS NULL) AND (cover_url IS NULL) = (cover_label IS NULL) AND (cover_url IS NULL) = (cover_source IS NULL)),
    CHECK ((source_tracks_kind = 'listed') = (source_tracks_count IS NOT NULL)),
    CHECK (source_tracks_total_ms IS NULL OR source_tracks_kind = 'listed')
) STRICT;

INSERT INTO import_candidate_match_v2 (
    content_hash, position, source, release_id, title, artist, year, format, label,
    catalog_number, country, barcode, cover_url, cover_thumbnail_url, cover_label,
    cover_source, source_group_id, source_tracks_kind, source_tracks_count,
    source_tracks_total_ms, by_disc_id, by_barcode, by_catalog, narrowed_out
)
SELECT content_hash, position, source, release_id, title, artist, year, format, label,
       catalog_number, country, barcode, cover_url, cover_thumbnail_url, cover_label,
       cover_source, source_group_id, source_tracks_kind, source_tracks_count,
       source_tracks_total_ms, by_disc_id, by_barcode, by_catalog, narrowed_out
FROM import_candidate_match;

DROP TABLE import_candidate_match;
ALTER TABLE import_candidate_match_v2 RENAME TO import_candidate_match;

DROP TABLE import_candidate_verdict;
ALTER TABLE import_candidate_verdict_v2 RENAME TO import_candidate_verdict;
