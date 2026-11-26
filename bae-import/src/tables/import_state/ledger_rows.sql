-- The catalogs the recorded ledger's run asked, in the order its cells are
-- laid out.
CREATE TABLE IF NOT EXISTS import_candidate_lookup_provider (
    verdict_id   INTEGER NOT NULL
        REFERENCES import_candidate_verdict (verdict_id) ON DELETE CASCADE,
    position     INTEGER NOT NULL CHECK (position >= 0),
    catalog      TEXT NOT NULL CHECK (catalog IN ('musicbrainz', 'discogs')),
    PRIMARY KEY (verdict_id, position),
    UNIQUE (verdict_id, catalog)
) STRICT;

-- The ISRCs the recorded ledger's run looked up together, each once, in the
-- files' order, for an isrc_step of 'read'.
CREATE TABLE IF NOT EXISTS import_candidate_lookup_isrc (
    verdict_id   INTEGER NOT NULL
        REFERENCES import_candidate_verdict (verdict_id) ON DELETE CASCADE,
    position     INTEGER NOT NULL CHECK (position >= 0),
    isrc         TEXT NOT NULL CHECK (isrc <> ''),
    PRIMARY KEY (verdict_id, position)
) STRICT;

-- The catalog numbers the folder's text offered that the recorded ledger's
-- run neither had in effect nor had struck out, in first-seen order, for a
-- catalog_step of 'numbers'.
CREATE TABLE IF NOT EXISTS import_candidate_lookup_offered_catalog (
    verdict_id   INTEGER NOT NULL
        REFERENCES import_candidate_verdict (verdict_id) ON DELETE CASCADE,
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT NOT NULL CHECK (value <> ''),
    PRIMARY KEY (verdict_id, position)
) STRICT;

-- The recorded ledger's rows, each clue's in its order: a disc ID, a barcode
-- or a catalog number (`value`), and the one row of the ISRCs and of the
-- title search, which name none. `excluded`: a barcode or catalog number the
-- person left out.
CREATE TABLE IF NOT EXISTS import_candidate_lookup_value (
    verdict_id   INTEGER NOT NULL
        REFERENCES import_candidate_verdict (verdict_id) ON DELETE CASCADE,
    step         TEXT NOT NULL CHECK (step IN ('disc_id', 'barcode', 'catalog', 'isrc', 'search')),
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT CHECK (value <> ''),
    excluded     INTEGER NOT NULL CHECK (excluded IN (0, 1)),
    PRIMARY KEY (verdict_id, step, position),
    CHECK ((step IN ('isrc', 'search')) = (value IS NULL)),
    CHECK (step NOT IN ('isrc', 'search') OR position = 0),
    CHECK (excluded = 0 OR step IN ('barcode', 'catalog'))
) STRICT;

-- One lookup of a ledger row: a cell, one catalog's answer, in the row's
-- cell order. A disc ID and the ISRCs are asked of MusicBrainz alone, so
-- their row has one lookup and names no catalog. 'answered': the releases it
-- found are import_candidate_lookup_release, none for no match.
CREATE TABLE IF NOT EXISTS import_candidate_lookup (
    verdict_id       INTEGER NOT NULL,
    step             TEXT NOT NULL,
    position         INTEGER NOT NULL,
    cell             INTEGER NOT NULL CHECK (cell >= 0),
    catalog          TEXT CHECK (catalog IN ('musicbrainz', 'discogs')),
    -- 'queued' and 'looking_up' only for a run in flight: a value not asked
    -- yet while the artwork is read, and a lookup under way.
    -- `asked_by` is then the run the verdict stands for, and NULL for every
    -- other answer.
    answer           TEXT NOT NULL
        CHECK (answer IN ('queued', 'looking_up', 'not_asked', 'answered', 'failed')),
    -- Why nobody was asked: the person left the value out, or no catalog the
    -- run asks answers this lookup.
    not_asked_reason TEXT CHECK (not_asked_reason IN ('left_out', 'no_catalog')),
    failure          TEXT CHECK (failure IN ('network', 'provider', 'timeout')),
    failure_status   INTEGER,
    asked_by         INTEGER,
    PRIMARY KEY (verdict_id, step, position, cell),
    -- Referenced by the releases found, which only an answered lookup has.
    UNIQUE (verdict_id, step, position, cell, answer),
    FOREIGN KEY (verdict_id, step, position)
        REFERENCES import_candidate_lookup_value (verdict_id, step, position) ON DELETE CASCADE,
    FOREIGN KEY (verdict_id, asked_by)
        REFERENCES import_candidate_verdict (verdict_id, identify_run),
    CHECK ((step IN ('disc_id', 'isrc')) = (catalog IS NULL)),
    CHECK (step NOT IN ('disc_id', 'isrc') OR cell = 0),
    CHECK ((answer = 'not_asked') = (not_asked_reason IS NOT NULL)),
    CHECK ((answer = 'failed') = (failure IS NOT NULL)),
    CHECK (failure_status IS NULL OR failure = 'provider'),
    CHECK ((answer IN ('queued', 'looking_up')) = (asked_by IS NOT NULL))
) STRICT;

-- The releases an answered lookup found, in its order, each the match row
-- that holds it.
CREATE TABLE IF NOT EXISTS import_candidate_lookup_release (
    verdict_id     INTEGER NOT NULL,
    step           TEXT NOT NULL,
    position       INTEGER NOT NULL,
    cell           INTEGER NOT NULL,
    answer         TEXT NOT NULL DEFAULT 'answered' CHECK (answer = 'answered'),
    ordinal        INTEGER NOT NULL CHECK (ordinal >= 0),
    match_position INTEGER NOT NULL,
    -- Where the release is in the cell's own list, laid out as a search's
    -- list is (see `import_candidate_match`): its row, its card, its place
    -- among its row's records, its row's section of the card and the row's
    -- place in that section.
    pressing       INTEGER NOT NULL CHECK (pressing >= 0),
    card           INTEGER NOT NULL CHECK (card >= 0),
    record_order   INTEGER NOT NULL CHECK (record_order >= 0),
    section        INTEGER NOT NULL CHECK (section >= 0),
    row_order      INTEGER NOT NULL CHECK (row_order >= 0),
    PRIMARY KEY (verdict_id, step, position, cell, ordinal),
    FOREIGN KEY (verdict_id, step, position, cell, answer)
        REFERENCES import_candidate_lookup (verdict_id, step, position, cell, answer)
        ON DELETE CASCADE,
    FOREIGN KEY (verdict_id, match_position)
        REFERENCES import_candidate_match (verdict_id, position)
) STRICT;
