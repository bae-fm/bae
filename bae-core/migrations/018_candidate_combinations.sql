ALTER TABLE scan_candidate ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'folder'
    CHECK (source_kind IN ('folder', 'combination'));

CREATE TABLE candidate_combination (
    candidate_key TEXT PRIMARY KEY,
    watched_folder_path TEXT NOT NULL
        REFERENCES watched_import_folders (path) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    track_order TEXT NOT NULL CHECK (track_order IN ('separate_discs', 'continuous')),
    skipped INTEGER NOT NULL DEFAULT 0 CHECK (skipped IN (0, 1)),
    created_at INTEGER NOT NULL,
    error TEXT
) STRICT;

-- Source keys are retained across scan-row replacement. Membership belongs to
-- the person's selection, not to a particular scan generation.
CREATE TABLE candidate_combination_member (
    combination_key TEXT NOT NULL
        REFERENCES candidate_combination (candidate_key) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    candidate_key TEXT NOT NULL UNIQUE,
    watched_folder_path TEXT NOT NULL,
    folder_name TEXT NOT NULL,
    file_prefix TEXT NOT NULL,
    first_disc INTEGER NOT NULL CHECK (first_disc >= 1),
    disc_count INTEGER NOT NULL CHECK (disc_count >= 1),
    track_count INTEGER NOT NULL CHECK (track_count >= 1),
    PRIMARY KEY (combination_key, position)
) STRICT;

CREATE INDEX candidate_combination_member_by_root
    ON candidate_combination_member (watched_folder_path);

-- Removing a watched root dissolves combinations that include it. Other
-- roots' source candidates and their drafts remain available independently.
CREATE TRIGGER remove_root_combinations BEFORE DELETE ON watched_import_folders
BEGIN
    DELETE FROM candidate_combination
    WHERE candidate_key IN (
        SELECT combination_key FROM candidate_combination_member
        WHERE watched_folder_path = OLD.path
    );
END;

-- A reviewed selection is never silently rebuilt from different files. A
-- changed or removed source blocks it until the person reviews its folders.
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

CREATE TRIGGER remove_combination_candidate AFTER DELETE ON candidate_combination
BEGIN
    DELETE FROM scan_candidate
    WHERE source_kind = 'combination' AND path = OLD.candidate_key;
END;
