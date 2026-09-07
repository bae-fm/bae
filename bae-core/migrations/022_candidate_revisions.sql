-- Whether discovery may initialize the draft is independent of its revision
-- identity. Preserve the previous initialization fact before versions are allocated.
ALTER TABLE import_candidate_state ADD COLUMN metadata_initialized INTEGER NOT NULL
    DEFAULT 0 CHECK (metadata_initialized IN (0, 1));
UPDATE import_candidate_state
SET metadata_initialized = metadata_revision != 0 OR EXISTS (
    SELECT 1 FROM import_candidate_draft_provenance AS provenance
    WHERE provenance.content_hash = import_candidate_state.content_hash
);

-- A candidate can disappear and later return with identical files. Its next
-- preparation must still have a version no previous operation could retain.
CREATE TABLE import_candidate_revision (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    last_revision INTEGER NOT NULL CHECK (last_revision >= 0)
) STRICT;
INSERT INTO import_candidate_revision (singleton, last_revision)
SELECT 1, COALESCE(MAX(metadata_revision), 0) FROM import_candidate_state;

-- Rebuild the header and its child together so the new revision is required,
-- including for future writes; no default can silently reuse an old version.
CREATE TABLE scan_candidate_tag_snapshot_with_revision (
    watched_folder_path TEXT NOT NULL,
    candidate_path TEXT NOT NULL,
    scan_generation INTEGER NOT NULL CHECK (scan_generation >= 0),
    file_edit_revision INTEGER NOT NULL CHECK (file_edit_revision >= 0),
    revision INTEGER NOT NULL UNIQUE CHECK (revision > 0),
    embedded_cover_source_relative_path TEXT,
    embedded_cover_content_type TEXT,
    embedded_cover_data BLOB,
    PRIMARY KEY (watched_folder_path, candidate_path),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE,
    CHECK (
        (embedded_cover_source_relative_path IS NULL
            AND embedded_cover_content_type IS NULL AND embedded_cover_data IS NULL)
        OR
        (embedded_cover_source_relative_path IS NOT NULL
            AND embedded_cover_content_type IS NOT NULL AND embedded_cover_data IS NOT NULL)
    )
) STRICT;
INSERT INTO scan_candidate_tag_snapshot_with_revision
    (watched_folder_path, candidate_path, scan_generation, file_edit_revision,
     revision, embedded_cover_source_relative_path, embedded_cover_content_type, embedded_cover_data)
SELECT watched_folder_path, candidate_path, scan_generation, file_edit_revision,
       (SELECT last_revision FROM import_candidate_revision WHERE singleton = 1)
           + ROW_NUMBER() OVER (ORDER BY watched_folder_path, candidate_path),
       embedded_cover_source_relative_path, embedded_cover_content_type, embedded_cover_data
FROM scan_candidate_tag_snapshot;
UPDATE import_candidate_revision
SET last_revision = last_revision + (SELECT COUNT(*) FROM scan_candidate_tag_snapshot);

CREATE TABLE scan_candidate_file_tag_with_revision (
    watched_folder_path TEXT NOT NULL,
    candidate_path TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    file_size INTEGER NOT NULL CHECK (file_size >= 0),
    modified_at_ns INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    title TEXT,
    track_artist TEXT,
    album_title TEXT,
    album_artist TEXT,
    year INTEGER,
    track_number INTEGER,
    disc_number INTEGER,
    PRIMARY KEY (watched_folder_path, candidate_path, relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate_tag_snapshot_with_revision (watched_folder_path, candidate_path) ON DELETE CASCADE,
    FOREIGN KEY (watched_folder_path, candidate_path, relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE
) STRICT;
INSERT INTO scan_candidate_file_tag_with_revision SELECT * FROM scan_candidate_file_tag;
DROP TABLE scan_candidate_file_tag;
DROP TABLE scan_candidate_tag_snapshot;
ALTER TABLE scan_candidate_tag_snapshot_with_revision RENAME TO scan_candidate_tag_snapshot;
ALTER TABLE scan_candidate_file_tag_with_revision RENAME TO scan_candidate_file_tag;
