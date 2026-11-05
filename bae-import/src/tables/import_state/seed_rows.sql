-- One folder being imported, keyed by the hash of its file layout. The other
-- import_candidate tables hang off it.
CREATE TABLE IF NOT EXISTS import_candidate_state (
    content_hash      TEXT PRIMARY KEY,
    -- The key the candidate was last seen under (`CandidateKey`): its
    -- folder's path, or its grouping's key. Not its identity.
    candidate_key     BLOB NOT NULL,
    -- Advances with every draft or cover change; commands return it so a
    -- surface can wait for exactly that committed state.
    metadata_revision INTEGER NOT NULL DEFAULT 0 CHECK (metadata_revision >= 0),
    -- Advances with every file decision, so a verdict based on older files is
    -- refused.
    edit_revision     INTEGER NOT NULL DEFAULT 0 CHECK (edit_revision >= 0)
) STRICT;

-- The candidates a file decision was made for, which a scan reads the
-- decisions of before it walks.
CREATE INDEX IF NOT EXISTS import_candidate_state_file_decided
    ON import_candidate_state (content_hash, edit_revision) WHERE edit_revision > 0;

-- The folders on disk a candidate was found at (several when the same files
-- sit in two places). The candidate lives while a watched folder covers one.
CREATE TABLE IF NOT EXISTS import_candidate_folder (
    content_hash TEXT NOT NULL,
    folder       BLOB NOT NULL,
    PRIMARY KEY (content_hash, folder),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The candidates found under a folder, which a watched folder's removal
-- forgets by the range of folders below it.
CREATE INDEX IF NOT EXISTS idx_import_candidate_folder_folder
    ON import_candidate_folder (folder);
