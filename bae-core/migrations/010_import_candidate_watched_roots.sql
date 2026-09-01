-- Candidate identity is content-based, so the same candidate can belong to
-- several watched roots. Keep those roots independently; a single last-seen
-- path cannot say which root still owns retained candidate state.
CREATE TABLE import_candidate_watched_root (
    content_hash        TEXT NOT NULL,
    watched_folder_path TEXT NOT NULL,
    PRIMARY KEY (content_hash, watched_folder_path),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path) ON DELETE CASCADE
) STRICT;

CREATE INDEX import_candidate_watched_root_by_root
    ON import_candidate_watched_root (watched_folder_path);

INSERT INTO import_candidate_watched_root (content_hash, watched_folder_path)
SELECT DISTINCT content_hash, watched_folder_path
FROM scan_candidate
WHERE content_hash IS NOT NULL;

-- Version nine retained candidate state after pruning its scan row, but did
-- not retain the watched root that owned it. Ownership cannot be reconstructed
-- from that row: folder_path may be the old root-relative display path. Drop
-- those unowned rows rather than attaching their edits to unrelated roots.
DELETE FROM import_candidate_state
WHERE NOT EXISTS (
    SELECT 1
    FROM import_candidate_watched_root AS ownership
    WHERE ownership.content_hash = import_candidate_state.content_hash
);
