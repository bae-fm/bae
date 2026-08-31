-- Exact provider bytes prepared for a candidate. Kept beside the existing
-- cover-selection row so upgrading preserves URL-only selections; a remote
-- selection without this row is explicitly unprepared and cannot import.
ALTER TABLE import_candidate_track_mapping ADD COLUMN source_position TEXT;
ALTER TABLE import_candidate_track_mapping ADD COLUMN file_author TEXT NOT NULL DEFAULT 'user'
    CHECK (file_author IN ('automatic', 'user'));

CREATE TABLE import_candidate_remote_cover_asset (
    content_hash TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    bytes        BLOB NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_cover (content_hash) ON DELETE CASCADE
) STRICT;

-- Presence means the candidate's current metadata revision has a complete
-- answer set, including the legitimate empty set. Migrated candidates have no
-- marker and must have their metadata source applied again before import.
CREATE TABLE import_candidate_asset_preparation (
    content_hash TEXT PRIMARY KEY,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

CREATE TABLE import_candidate_source_artist (
    content_hash        TEXT NOT NULL,
    discogs_artist_id   TEXT NOT NULL,
    PRIMARY KEY (content_hash, discogs_artist_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

CREATE TABLE import_candidate_artist_asset (
    content_hash        TEXT NOT NULL,
    discogs_artist_id   TEXT NOT NULL,
    answer              TEXT NOT NULL CHECK (answer IN ('image', 'nothing')),
    source_url          TEXT,
    content_type        TEXT,
    bytes               BLOB,
    PRIMARY KEY (content_hash, discogs_artist_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK (
        (answer = 'image' AND source_url IS NOT NULL AND content_type IS NOT NULL AND bytes IS NOT NULL)
        OR
        (answer = 'nothing' AND source_url IS NULL AND content_type IS NULL AND bytes IS NULL)
    )
) STRICT;
