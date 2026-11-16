-- This table records the audio file the person bound each `FILE` reference
-- of a release's CUE sheet to in the import pane, or that they cleared the
-- reference. When bae identifies the release again, it binds the sheets the
-- same way. The sheet and the audio are the release's own files, and
-- deleting either file deletes its rows here. Only a reference the person
-- decided has a row, and the scan binds every other reference.
CREATE TABLE IF NOT EXISTS release_sheet_bindings (
    sheet_file_id  TEXT NOT NULL,
    file_reference TEXT NOT NULL,
    audio_file_id  TEXT,
    PRIMARY KEY (sheet_file_id, file_reference),
    FOREIGN KEY (sheet_file_id) REFERENCES release_files (id) ON DELETE CASCADE,
    FOREIGN KEY (audio_file_id) REFERENCES release_files (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_sheet_bindings_audio
    ON release_sheet_bindings (audio_file_id) WHERE audio_file_id IS NOT NULL;

-- This table records which disc of the release the person said each CUE
-- sheet is, or that the release ignores the sheet. Only a sheet the person
-- decided has a row, and the scan gives every other sheet its disc.
CREATE TABLE IF NOT EXISTS release_sheet_discs (
    sheet_file_id TEXT NOT NULL PRIMARY KEY,
    disc          TEXT NOT NULL CHECK (disc IN ('disc', 'ignored')),
    disc_number   INTEGER CHECK (disc_number IS NULL OR disc_number >= 1),
    FOREIGN KEY (sheet_file_id) REFERENCES release_files (id) ON DELETE CASCADE,
    CHECK ((disc = 'disc') = (disc_number IS NOT NULL))
) STRICT;
