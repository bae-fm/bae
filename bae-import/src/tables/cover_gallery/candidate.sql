-- These tables hold, for each candidate, the covers its link's catalogs
-- offered the last time this device fetched them for the cover picker. One
-- `candidate_cover_fetch` row per candidate holds the link the fetch read the
-- covers for (`candidate_cover_link`'s JSON, NULL for no link) and whether
-- the candidate was linked then. A fetch that found it linked adds its covers
-- as `candidate_cover_entry` rows, in the order offered, each the stored
-- `RemoteCover` as JSON with its catalog beside it for the picker's source
-- filter. The picker shows a fetch's covers only while the candidate's link
-- is the one the fetch read them for. Device-local: any device can fetch. A
-- later fetch replaces the candidate's rows whole; deleting the candidate's
-- state deletes its rows.
CREATE TABLE IF NOT EXISTS candidate_cover_fetch (
    content_hash TEXT NOT NULL PRIMARY KEY
        REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    link         TEXT CHECK (link IS NULL OR json_valid(link)),
    linked       INTEGER NOT NULL CHECK (linked IN (0, 1))
) STRICT;

CREATE TABLE IF NOT EXISTS candidate_cover_entry (
    content_hash TEXT NOT NULL
        REFERENCES candidate_cover_fetch (content_hash) ON DELETE CASCADE,
    position     INTEGER NOT NULL CHECK (position >= 0),
    source       TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    cover        TEXT NOT NULL CHECK (json_valid(cover)),
    PRIMARY KEY (content_hash, position)
) STRICT;
