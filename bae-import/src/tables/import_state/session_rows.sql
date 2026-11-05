-- What the import pane is showing for this candidate and what is typed in its
-- search fields.
CREATE TABLE IF NOT EXISTS import_candidate_session (
    content_hash   TEXT PRIMARY KEY,
    presentation   TEXT NOT NULL CHECK (presentation IN ('draft', 'find_online')),
    find_online_section TEXT NOT NULL CHECK (find_online_section IN ('automatic', 'search')),
    search_tab     TEXT NOT NULL CHECK (search_tab IN ('general', 'catalog_number', 'barcode')),
    search_artist  TEXT NOT NULL,
    search_album   TEXT NOT NULL,
    search_catalog TEXT NOT NULL,
    search_barcode TEXT NOT NULL,
    -- The pane's last command, when it failed: which command, the class of
    -- its failure, and the failure's untranslated text.
    error_command  TEXT CHECK (error_command IN (
        'import', 'read_file_tags', 'change_lookups', 'change_search_words',
        'change_agreements', 'keep_own_draft', 'link_shared_album', 'unlink')),
    error_category TEXT,
    error_detail   TEXT,
    CHECK ((error_command IS NULL) = (error_category IS NULL)),
    CHECK ((error_command IS NULL) = (error_detail IS NULL)),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;
