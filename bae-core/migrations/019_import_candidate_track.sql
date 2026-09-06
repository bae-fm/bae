-- A candidate draft has exactly one track per audio slot, and every track has
-- exactly one physical decision: which file plays it, who chose that file,
-- whether the source named the track, and whether it is still in the import.
-- Two tables kept those halves apart, joined by track id, and nothing in the
-- schema said the two sets were the same set — a file decision that grew the
-- slots wrote mapping rows for tracks the draft did not have. One row per
-- track makes that unrepresentable.

CREATE TABLE import_candidate_track (
    content_hash           TEXT NOT NULL,
    track_id               TEXT NOT NULL,
    position               INTEGER NOT NULL CHECK (position >= 0),
    title                  TEXT NOT NULL,
    artist_assignment_kind TEXT NOT NULL CHECK (artist_assignment_kind IN ('album_artists', 'explicit')),
    side                   INTEGER NOT NULL,
    track_number           INTEGER,
    named_by_source        INTEGER NOT NULL CHECK (named_by_source IN (0, 1)),
    dropped                INTEGER NOT NULL CHECK (dropped IN (0, 1)),
    file_author            TEXT NOT NULL CHECK (file_author IN ('automatic', 'user')),
    file_kind              TEXT CHECK (file_kind IS NULL OR file_kind IN ('standalone', 'sheet_slice')),
    file_id                TEXT,
    sheet_id               TEXT,
    slice_index            INTEGER CHECK (slice_index IS NULL OR slice_index >= 0),
    PRIMARY KEY (content_hash, track_id),
    UNIQUE (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    -- A row out of the import plays nothing, so it names nothing.
    CHECK (dropped = 0 OR file_kind IS NULL),
    CHECK (
        (file_kind IS NULL AND file_id IS NULL AND sheet_id IS NULL AND slice_index IS NULL)
        OR (file_kind = 'standalone' AND file_id IS NOT NULL AND sheet_id IS NULL AND slice_index IS NULL)
        OR (file_kind = 'sheet_slice' AND file_id IS NOT NULL AND sheet_id IS NOT NULL AND slice_index IS NOT NULL)
    )
) STRICT;

-- Every draft track row carries over. A track whose mapping row was missing
-- takes the decision a fresh projection would give it: in the import, bound
-- automatically to nothing, named by its source. A mapping row with no draft
-- track was the inconsistency this table exists to refuse, and is dropped.
INSERT INTO import_candidate_track (
    content_hash, track_id, position, title, artist_assignment_kind, side, track_number,
    named_by_source, dropped, file_author, file_kind, file_id, sheet_id, slice_index
)
SELECT
    edit.content_hash, edit.track_id, edit.position, edit.title, edit.artist_assignment_kind,
    edit.side, edit.track_number,
    COALESCE(mapping.named_by_source, 1),
    COALESCE(mapping.dropped, 0),
    COALESCE(mapping.file_author, 'automatic'),
    mapping.file_kind, mapping.file_id, mapping.sheet_id, mapping.slice_index
FROM import_candidate_track_edit edit
LEFT JOIN import_candidate_track_mapping mapping
    ON mapping.content_hash = edit.content_hash AND mapping.track_id = edit.track_id;

-- The track artist assignments hang off the track row. SQLite cannot repoint a
-- foreign key, so the table is rebuilt against the merged one.
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
        REFERENCES import_candidate_track (content_hash, track_id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE RESTRICT,
    CHECK (
        (assignment_kind = 'existing' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL)
        OR
        (assignment_kind = 'new' AND artist_id IS NULL AND name IS NOT NULL AND name <> '')
    )
) STRICT;

INSERT INTO import_candidate_track_artist_assignment_v2
SELECT content_hash, track_id, position, assignment_kind, artist_id, name, sort_name,
       musicbrainz_artist_id, discogs_artist_id
FROM import_candidate_track_artist_assignment;

DROP TABLE import_candidate_track_artist_assignment;
ALTER TABLE import_candidate_track_artist_assignment_v2
    RENAME TO import_candidate_track_artist_assignment;

DROP TABLE import_candidate_track_mapping;
DROP TABLE import_candidate_track_edit;
