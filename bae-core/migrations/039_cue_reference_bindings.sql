-- Each CUE FILE reference has its own decision. Selection remains on the sheet.
CREATE TABLE import_candidate_sheet_reference (
    content_hash TEXT NOT NULL,
    sheet_id TEXT NOT NULL,
    file_reference TEXT NOT NULL,
    file_id TEXT,
    PRIMARY KEY (content_hash, sheet_id, file_reference),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state(content_hash) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_sheet_reference
    (content_hash, sheet_id, file_reference, file_id)
SELECT e.content_hash, e.relative_path,
    (SELECT MIN(t.file_reference) FROM scan_candidate c JOIN scan_cue_track t
        ON t.watched_folder_path = c.watched_folder_path AND t.candidate_path = c.path
        WHERE c.content_hash = e.content_hash AND t.sheet_relative_path = e.relative_path),
    e.sheet_binding_file_id
FROM import_candidate_file_edit e WHERE e.sheet_binding IS NOT NULL;

CREATE TABLE import_candidate_file_edit_v3 (
    content_hash TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    role_choice TEXT CHECK (role_choice IS NULL OR role_choice IN ('audio', 'not_a_track')),
    sheet_disc TEXT CHECK (sheet_disc IS NULL OR sheet_disc IN ('disc', 'ignored')),
    sheet_disc_number INTEGER CHECK (sheet_disc_number IS NULL OR sheet_disc_number >= 1),
    PRIMARY KEY (content_hash, relative_path),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state(content_hash) ON DELETE CASCADE,
    CHECK ((sheet_disc = 'disc') = (sheet_disc_number IS NOT NULL)),
    CHECK (role_choice IS NOT NULL OR sheet_disc IS NOT NULL)
) STRICT;
INSERT INTO import_candidate_file_edit_v3
SELECT content_hash, relative_path, role_choice, sheet_disc, sheet_disc_number
FROM import_candidate_file_edit WHERE role_choice IS NOT NULL OR sheet_disc IS NOT NULL;
DROP TABLE import_candidate_file_edit;
ALTER TABLE import_candidate_file_edit_v3 RENAME TO import_candidate_file_edit;
UPDATE scan_candidate_file SET sheet_binding = 'resolved' WHERE sheet_binding = 'override';
