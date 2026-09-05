-- What the pane keeps per candidate between visits: which metadata surface it
-- is showing, the typed-search form, and the last command that failed. One
-- row per candidate, written as the person works and read back with the rest
-- of the candidate, so clicking away and a relaunch both come back to the same
-- pane. No row means the pane has not been touched: it opens on its initial
-- surface with an empty form.
CREATE TABLE import_candidate_session (
    content_hash   TEXT PRIMARY KEY,
    presentation   TEXT NOT NULL CHECK (presentation IN ('draft', 'find_online', 'file_tags')),
    search_tab     TEXT NOT NULL CHECK (search_tab IN ('general', 'catalog_number', 'barcode')),
    search_artist  TEXT NOT NULL,
    search_album   TEXT NOT NULL,
    search_catalog TEXT NOT NULL,
    search_barcode TEXT NOT NULL,
    error          TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;
