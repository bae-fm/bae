-- The candidate row no longer stamps which source a candidate started from.
-- Whether a draft is pre-filled from the folder's file tags, and whether
-- identification runs on its own, are two library preferences read when the
-- work happens; neither is a fact about a scanned folder.
--
-- The column is named by one of the table's CHECK constraints, so SQLite
-- cannot drop it in place and the table is rebuilt. Dropping a parent runs an
-- implicit delete that fires ON DELETE CASCADE down the scan tables, so every
-- descendant's rows are held in temporary tables across the swap and put back
-- afterwards: a scan candidate's files, tag snapshot and boundaries survive
-- the rebuild.
CREATE TEMP TABLE scan_candidate_carry AS
    SELECT watched_folder_path, path, generation, kind, name, display_path, file_root,
           scope, content_hash, file_edit_revision, combine_ancestor_relative_path,
           invalid_reason, invalid_reason_path, first_seen_at, source_date,
           source_date_kind, source_kind
    FROM scan_candidate;
CREATE TEMP TABLE scan_candidate_file_carry AS SELECT * FROM scan_candidate_file;
CREATE TEMP TABLE scan_candidate_tag_snapshot_carry AS SELECT * FROM scan_candidate_tag_snapshot;
CREATE TEMP TABLE scan_candidate_file_tag_carry AS SELECT * FROM scan_candidate_file_tag;
CREATE TEMP TABLE scan_candidate_resolved_boundary_carry AS SELECT * FROM scan_candidate_resolved_boundary;
CREATE TEMP TABLE scan_cue_sheet_carry AS SELECT * FROM scan_cue_sheet;
CREATE TEMP TABLE scan_cue_track_carry AS SELECT * FROM scan_cue_track;
CREATE TEMP TABLE scan_sheet_audio_file_carry AS SELECT * FROM scan_sheet_audio_file;

DROP TABLE scan_candidate;

CREATE TABLE scan_candidate (
    watched_folder_path            TEXT NOT NULL,
    path                           TEXT NOT NULL,
    generation                     INTEGER NOT NULL CHECK (generation >= 0),
    kind                           TEXT NOT NULL CHECK (kind IN ('tentative', 'valid', 'invalid')),
    name                           TEXT NOT NULL,
    display_path                   TEXT NOT NULL,
    file_root                      TEXT,
    scope                          TEXT CHECK (scope IS NULL OR scope IN ('direct', 'recursive')),
    content_hash                   TEXT,
    file_edit_revision             INTEGER NOT NULL DEFAULT 0 CHECK (file_edit_revision >= 0),
    combine_ancestor_relative_path TEXT,
    invalid_reason                 TEXT CHECK (invalid_reason IS NULL OR invalid_reason IN ('corrupt_audio', 'corrupt_image', 'no_valid_audio')),
    invalid_reason_path            TEXT,
    first_seen_at                  INTEGER,
    source_date                    INTEGER,
    source_date_kind               TEXT CHECK ((source_date IS NULL AND source_date_kind IS NULL)
        OR (source_date IS NOT NULL AND source_date_kind IS NOT NULL
            AND source_date_kind IN ('added_to_directory', 'created'))),
    source_kind                    TEXT NOT NULL DEFAULT 'folder' CHECK (source_kind IN ('folder', 'combination')),
    PRIMARY KEY (watched_folder_path, path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE,
    CHECK ((kind = 'invalid') = (invalid_reason IS NOT NULL)),
    CHECK ((kind = 'invalid') = (file_root IS NULL AND scope IS NULL AND content_hash IS NULL)),
    CHECK ((invalid_reason IN ('corrupt_audio', 'corrupt_image')) = (invalid_reason_path IS NOT NULL))
) STRICT;

CREATE INDEX idx_scan_candidate_path ON scan_candidate (path);
CREATE INDEX idx_scan_candidate_content_hash
    ON scan_candidate (content_hash) WHERE content_hash IS NOT NULL;

CREATE TRIGGER invalidate_combination_source_delete BEFORE DELETE ON scan_candidate
WHEN OLD.source_kind = 'folder'
BEGIN
    UPDATE candidate_combination
    SET error = 'Source folder changed or disappeared: ' || OLD.name
    WHERE candidate_key IN (
        SELECT combination_key FROM candidate_combination_member WHERE candidate_key = OLD.path
    );
END;

CREATE TRIGGER invalidate_combination_source_edit AFTER UPDATE OF content_hash, file_edit_revision ON scan_candidate
WHEN OLD.source_kind = 'folder'
    AND (NEW.content_hash IS NOT OLD.content_hash OR NEW.file_edit_revision != OLD.file_edit_revision)
BEGIN
    UPDATE candidate_combination
    SET error = 'Source folder changed: ' || OLD.name
    WHERE candidate_key IN (
        SELECT combination_key FROM candidate_combination_member WHERE candidate_key = OLD.path
    );
END;

INSERT INTO scan_candidate SELECT * FROM scan_candidate_carry;
INSERT INTO scan_candidate_file SELECT * FROM scan_candidate_file_carry;
INSERT INTO scan_candidate_tag_snapshot SELECT * FROM scan_candidate_tag_snapshot_carry;
INSERT INTO scan_candidate_file_tag SELECT * FROM scan_candidate_file_tag_carry;
INSERT INTO scan_candidate_resolved_boundary SELECT * FROM scan_candidate_resolved_boundary_carry;
INSERT INTO scan_cue_sheet SELECT * FROM scan_cue_sheet_carry;
INSERT INTO scan_sheet_audio_file SELECT * FROM scan_sheet_audio_file_carry;
INSERT INTO scan_cue_track SELECT * FROM scan_cue_track_carry;

DROP TABLE scan_candidate_carry;
DROP TABLE scan_candidate_file_carry;
DROP TABLE scan_candidate_tag_snapshot_carry;
DROP TABLE scan_candidate_file_tag_carry;
DROP TABLE scan_candidate_resolved_boundary_carry;
DROP TABLE scan_cue_sheet_carry;
DROP TABLE scan_cue_track_carry;
DROP TABLE scan_sheet_audio_file_carry;
