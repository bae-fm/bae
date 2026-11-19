-- The import pane's last command for a candidate, when it failed: which
-- command, the class of its failure, and the failure's untranslated text. The
-- pane's next command deletes the row.
CREATE TABLE IF NOT EXISTS import_candidate_pane_failure (
    content_hash TEXT PRIMARY KEY,
    command      TEXT NOT NULL CHECK (command IN (
        'import', 'read_file_tags', 'change_lookups', 'change_search_words',
        'change_agreements', 'keep_own_draft', 'link_shared_album', 'unlink')),
    category     TEXT NOT NULL,
    detail       TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;
