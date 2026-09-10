-- The pane has no File tags surface: the folder's own tags reach the draft
-- through "Reset to tags", which replaces the draft and returns, rather than
-- through a browser the pane sits on. A session that was left on that surface
-- opens on the draft, which is where its metadata is, and no session can name
-- the surface again.
CREATE TABLE import_candidate_session_rebuilt (
    content_hash   TEXT PRIMARY KEY,
    presentation   TEXT NOT NULL CHECK (presentation IN ('draft', 'find_online')),
    search_tab     TEXT NOT NULL CHECK (search_tab IN ('general', 'catalog_number', 'barcode')),
    search_artist  TEXT NOT NULL,
    search_album   TEXT NOT NULL,
    search_catalog TEXT NOT NULL,
    search_barcode TEXT NOT NULL,
    error          TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_session_rebuilt
    SELECT content_hash,
           CASE presentation WHEN 'file_tags' THEN 'draft' ELSE presentation END,
           search_tab, search_artist, search_album, search_catalog, search_barcode, error
    FROM import_candidate_session;

DROP TABLE import_candidate_session;
ALTER TABLE import_candidate_session_rebuilt RENAME TO import_candidate_session;
