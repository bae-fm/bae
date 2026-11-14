-- A person's typed search for a candidate, one per key: what was asked, as
-- columns by mode. Each catalog's part of it is an
-- import_candidate_search_source row, and the releases a catalog returned are
-- import_candidate_match rows owned by `search_run`. A new search for the key
-- replaces the row, and with it every row it owns; `search_run` is never
-- reused, so a catalog's answer to a replaced search lands nowhere. A folder
-- candidate's search is of the files it names, and goes in the scan write
-- that leaves the candidate with other files or none; a library release's
-- names none. No search outlives the process, so the table is emptied as the
-- library opens.
CREATE TABLE IF NOT EXISTS import_candidate_search (
    search_run         INTEGER PRIMARY KEY AUTOINCREMENT,
    candidate_key      BLOB NOT NULL UNIQUE,
    content_hash       TEXT,
    file_edit_revision INTEGER CHECK (file_edit_revision >= 0),
    mode               TEXT NOT NULL CHECK (mode IN ('general', 'catalog_number', 'barcode')),
    artist             TEXT,
    album              TEXT,
    catalog_number     TEXT,
    barcode            TEXT,
    CHECK ((content_hash IS NULL) = (file_edit_revision IS NULL)),
    CHECK ((mode = 'general') = (artist IS NOT NULL)),
    CHECK ((mode = 'general') = (album IS NOT NULL)),
    CHECK ((mode = 'catalog_number') = (catalog_number IS NOT NULL)),
    CHECK ((mode = 'barcode') = (barcode IS NOT NULL))
) STRICT;

-- One catalog's part of a search, at its place in the order the catalogs are
-- listed. 'off': the person is not asking it; 'not_configured': it needs a
-- credential the library does not hold; neither was asked. 'searching': its
-- answer is out; 'done': it answered, with the releases it returned as the
-- search's match rows; 'failed': it could not answer (`failure`, with the
-- provider's HTTP status), or bae broke asking it (`error_detail`, the error
-- chain the person is shown). Only a part still searching takes an answer.
CREATE TABLE IF NOT EXISTS import_candidate_search_source (
    search_run     INTEGER NOT NULL
        REFERENCES import_candidate_search (search_run) ON DELETE CASCADE,
    catalog        TEXT NOT NULL CHECK (catalog IN ('musicbrainz', 'discogs')),
    position       INTEGER NOT NULL CHECK (position >= 0),
    state          TEXT NOT NULL
        CHECK (state IN ('off', 'not_configured', 'searching', 'done', 'failed')),
    failure        TEXT CHECK (failure IN ('network', 'provider', 'timeout')),
    failure_status INTEGER,
    error_detail   TEXT CHECK (error_detail <> ''),
    PRIMARY KEY (search_run, catalog),
    UNIQUE (search_run, position),
    -- Referenced by the releases a part returned, which only a part that
    -- answered has.
    UNIQUE (search_run, catalog, state),
    CHECK ((state = 'failed') = (failure IS NOT NULL OR error_detail IS NOT NULL)),
    CHECK (failure IS NULL OR error_detail IS NULL),
    CHECK (failure_status IS NULL OR failure = 'provider')
) STRICT;

-- What a pick of a MusicBrainz release read its album to be on the other
-- catalog: the release's own documents' statements, before the list's
-- releases are compared. A search's rows state what each group was read to be
-- from all its picks together and the list (`import_candidate_match.album_links`),
-- and a pick is the input of that reading, so it is kept apart. A pick lasts
-- as long as the search holds the release.
CREATE TABLE IF NOT EXISTS import_candidate_search_pick (
    search_run  INTEGER NOT NULL
        REFERENCES import_candidate_search (search_run) ON DELETE CASCADE,
    release_id  TEXT NOT NULL CHECK (release_id <> ''),
    -- As `import_candidate_match.album_links`: the album rows hold what the
    -- documents named.
    links       TEXT NOT NULL CHECK (links IN ('not_asked', 'read', 'unread')),
    PRIMARY KEY (search_run, release_id)
) STRICT;

-- The albums a pick's documents named, strongest first.
CREATE TABLE IF NOT EXISTS import_candidate_search_pick_album (
    search_run  INTEGER NOT NULL,
    release_id  TEXT NOT NULL,
    ordinal     INTEGER NOT NULL CHECK (ordinal >= 0),
    catalog     TEXT NOT NULL CHECK (catalog <> ''),
    key         TEXT NOT NULL CHECK (key <> ''),
    PRIMARY KEY (search_run, release_id, ordinal),
    FOREIGN KEY (search_run, release_id)
        REFERENCES import_candidate_search_pick (search_run, release_id) ON DELETE CASCADE
) STRICT;
