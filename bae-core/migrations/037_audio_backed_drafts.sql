-- The included tracks require audio. Retain valid stored edits and their
-- source positions; remove exclusions and metadata-only rows.
CREATE TABLE import_candidate_track_v2 (
    content_hash           TEXT NOT NULL,
    track_id               TEXT NOT NULL,
    position               INTEGER NOT NULL CHECK (position >= 0),
    title                  TEXT NOT NULL,
    artist_assignment_kind TEXT NOT NULL CHECK (artist_assignment_kind IN ('album_artists', 'explicit')),
    side                   INTEGER,
    track_number           INTEGER NOT NULL,
    source_index           INTEGER CHECK (source_index IS NULL OR source_index >= 0),
    file_kind              TEXT NOT NULL CHECK (file_kind IN ('standalone', 'sheet_slice')),
    file_id                TEXT NOT NULL,
    sheet_id               TEXT,
    slice_index            INTEGER CHECK (slice_index IS NULL OR slice_index >= 0),
    PRIMARY KEY (content_hash, track_id),
    UNIQUE (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    CHECK (
        (file_kind = 'standalone' AND file_id IS NOT NULL AND sheet_id IS NULL AND slice_index IS NULL)
        OR (file_kind = 'sheet_slice' AND file_id IS NOT NULL AND sheet_id IS NOT NULL AND slice_index IS NOT NULL)
    )
) STRICT;


INSERT INTO import_candidate_track_v2
SELECT content_hash, track_id, position, title, artist_assignment_kind, side,
       COALESCE(track_number, position + 1),
       CASE WHEN named_by_source = 1 AND EXISTS (
           SELECT 1 FROM import_candidate_draft_provenance provenance
           WHERE provenance.content_hash = import_candidate_track.content_hash
             AND provenance.kind = 'external_release'
       ) THEN position ELSE NULL END,
       file_kind, file_id, sheet_id, slice_index
FROM import_candidate_track
WHERE dropped = 0 AND file_kind IS NOT NULL;

CREATE TABLE import_candidate_track_artist_assignment_v2 (
    content_hash          TEXT NOT NULL,
    track_id              TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('existing', 'new')),
    artist_id             TEXT,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (content_hash, track_id, position),
    FOREIGN KEY (content_hash, track_id)
        REFERENCES import_candidate_track_v2 (content_hash, track_id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE RESTRICT,
    CHECK (
        (assignment_kind = 'existing' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL)
        OR
        (assignment_kind = 'new' AND artist_id IS NULL AND name IS NOT NULL AND name <> '')
    )
) STRICT;


INSERT INTO import_candidate_track_artist_assignment_v2
SELECT assignment.* FROM import_candidate_track_artist_assignment assignment
JOIN import_candidate_track_v2 track USING (content_hash, track_id);

DROP TABLE import_candidate_track_artist_assignment;
DROP TABLE import_candidate_track;
ALTER TABLE import_candidate_track_v2 RENAME TO import_candidate_track;
ALTER TABLE import_candidate_track_artist_assignment_v2
    RENAME TO import_candidate_track_artist_assignment;
