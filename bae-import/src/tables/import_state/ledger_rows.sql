-- The catalogs the recorded ledger's run asked, in the order its cells are
-- laid out.
CREATE TABLE IF NOT EXISTS import_candidate_lookup_provider (
    content_hash TEXT,
    identify_run INTEGER,
    position     INTEGER NOT NULL CHECK (position >= 0),
    catalog      TEXT NOT NULL CHECK (catalog IN ('musicbrainz', 'discogs')),
    UNIQUE (content_hash, position),
    UNIQUE (identify_run, position),
    UNIQUE (content_hash, catalog),
    UNIQUE (identify_run, catalog),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_verdict (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (identify_run) REFERENCES import_candidate_verdict (identify_run) ON DELETE CASCADE,
    CHECK ((content_hash IS NULL) <> (identify_run IS NULL))
) STRICT;

-- The ISRCs the recorded ledger's run looked up together, each once, in the
-- files' order, for an isrc_step of 'read'.
CREATE TABLE IF NOT EXISTS import_candidate_lookup_isrc (
    content_hash TEXT,
    identify_run INTEGER,
    position     INTEGER NOT NULL CHECK (position >= 0),
    isrc         TEXT NOT NULL CHECK (isrc <> ''),
    UNIQUE (content_hash, position),
    UNIQUE (identify_run, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_verdict (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (identify_run) REFERENCES import_candidate_verdict (identify_run) ON DELETE CASCADE,
    CHECK ((content_hash IS NULL) <> (identify_run IS NULL))
) STRICT;

-- The catalog numbers the folder's text offered that the recorded ledger's
-- run neither had in effect nor had struck out, in first-seen order, for a
-- catalog_step of 'numbers'.
CREATE TABLE IF NOT EXISTS import_candidate_lookup_offered_catalog (
    content_hash TEXT,
    identify_run INTEGER,
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT NOT NULL CHECK (value <> ''),
    UNIQUE (content_hash, position),
    UNIQUE (identify_run, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_verdict (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (identify_run) REFERENCES import_candidate_verdict (identify_run) ON DELETE CASCADE,
    CHECK ((content_hash IS NULL) <> (identify_run IS NULL))
) STRICT;

-- The recorded ledger's rows, each clue's in its order: a disc ID, a barcode
-- or a catalog number (`value`), and the one row of the ISRCs and of the
-- title search, which name none. `excluded`: a barcode or catalog number the
-- person left out.
CREATE TABLE IF NOT EXISTS import_candidate_lookup_value (
    content_hash TEXT,
    identify_run INTEGER,
    step         TEXT NOT NULL CHECK (step IN ('disc_id', 'barcode', 'catalog', 'isrc', 'search')),
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT CHECK (value <> ''),
    excluded     INTEGER NOT NULL CHECK (excluded IN (0, 1)),
    UNIQUE (content_hash, step, position),
    UNIQUE (identify_run, step, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_verdict (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (identify_run) REFERENCES import_candidate_verdict (identify_run) ON DELETE CASCADE,
    CHECK ((step IN ('isrc', 'search')) = (value IS NULL)),
    CHECK (step NOT IN ('isrc', 'search') OR position = 0),
    CHECK (excluded = 0 OR step IN ('barcode', 'catalog')),
    CHECK ((content_hash IS NULL) <> (identify_run IS NULL))
) STRICT;

-- One lookup of a ledger row: a cell, one catalog's answer, in the row's
-- cell order. A disc ID and the ISRCs are asked of MusicBrainz alone, so
-- their row has one lookup and names no catalog. 'answered': the releases it
-- found are import_candidate_lookup_release, none for no match.
CREATE TABLE IF NOT EXISTS import_candidate_lookup (
    content_hash     TEXT,
    identify_run     INTEGER,
    step             TEXT NOT NULL,
    position         INTEGER NOT NULL,
    cell             INTEGER NOT NULL CHECK (cell >= 0),
    catalog          TEXT CHECK (catalog IN ('musicbrainz', 'discogs')),
    -- 'queued' and 'looking_up' only for a run in flight: a value not asked
    -- yet while the artwork is read, and a lookup under way.
    answer           TEXT NOT NULL
        CHECK (answer IN ('queued', 'looking_up', 'not_asked', 'answered', 'failed')),
    -- Why nobody was asked: the person left the value out, or no catalog the
    -- run asks answers this lookup.
    not_asked_reason TEXT CHECK (not_asked_reason IN ('left_out', 'no_catalog')),
    failure          TEXT CHECK (failure IN ('network', 'provider', 'timeout')),
    failure_status   INTEGER,
    UNIQUE (content_hash, step, position, cell),
    UNIQUE (identify_run, step, position, cell),
    -- Referenced by the releases found, which only an answered lookup has.
    UNIQUE (content_hash, step, position, cell, answer),
    UNIQUE (identify_run, step, position, cell, answer),
    FOREIGN KEY (content_hash, step, position)
        REFERENCES import_candidate_lookup_value (content_hash, step, position) ON DELETE CASCADE,
    FOREIGN KEY (identify_run, step, position)
        REFERENCES import_candidate_lookup_value (identify_run, step, position) ON DELETE CASCADE,
    CHECK ((step IN ('disc_id', 'isrc')) = (catalog IS NULL)),
    CHECK (step NOT IN ('disc_id', 'isrc') OR cell = 0),
    CHECK ((answer = 'not_asked') = (not_asked_reason IS NOT NULL)),
    CHECK ((answer = 'failed') = (failure IS NOT NULL)),
    CHECK (failure_status IS NULL OR failure = 'provider'),
    CHECK (answer NOT IN ('queued', 'looking_up') OR identify_run IS NOT NULL),
    CHECK ((content_hash IS NULL) <> (identify_run IS NULL))
) STRICT;

-- The releases an answered lookup found, in its order, each the match row
-- that holds it.
CREATE TABLE IF NOT EXISTS import_candidate_lookup_release (
    content_hash   TEXT,
    identify_run   INTEGER,
    step           TEXT NOT NULL,
    position       INTEGER NOT NULL,
    cell           INTEGER NOT NULL,
    answer         TEXT NOT NULL DEFAULT 'answered' CHECK (answer = 'answered'),
    ordinal        INTEGER NOT NULL CHECK (ordinal >= 0),
    match_position INTEGER NOT NULL,
    UNIQUE (content_hash, step, position, cell, ordinal),
    UNIQUE (identify_run, step, position, cell, ordinal),
    FOREIGN KEY (content_hash, step, position, cell, answer)
        REFERENCES import_candidate_lookup (content_hash, step, position, cell, answer)
        ON DELETE CASCADE,
    FOREIGN KEY (identify_run, step, position, cell, answer)
        REFERENCES import_candidate_lookup (identify_run, step, position, cell, answer)
        ON DELETE CASCADE,
    FOREIGN KEY (content_hash, match_position)
        REFERENCES import_candidate_match (content_hash, position),
    FOREIGN KEY (identify_run, match_position)
        REFERENCES import_candidate_match (identify_run, position),
    CHECK ((content_hash IS NULL) <> (identify_run IS NULL))
) STRICT;
