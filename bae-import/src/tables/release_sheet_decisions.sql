-- The audio the person bound each `FILE` reference of a release's CUE sheet to
-- in the import pane, or cleared (no audio), kept with the release so
-- identifying it again binds its sheets as the person did. Sheet and audio are
-- the release's own files; a deleted file takes its bindings with it. Only a
-- reference the person decided has a row: any other is bound as the scan binds it.
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

-- Which disc of the release the person said each CUE sheet is, or that the
-- release ignores it, kept with the release as the bindings are. Only a sheet
-- the person decided has a row: any other takes its disc by the scan's rules.
CREATE TABLE IF NOT EXISTS release_sheet_discs (
    sheet_file_id TEXT NOT NULL PRIMARY KEY,
    disc          TEXT NOT NULL CHECK (disc IN ('disc', 'ignored')),
    disc_number   INTEGER CHECK (disc_number IS NULL OR disc_number >= 1),
    FOREIGN KEY (sheet_file_id) REFERENCES release_files (id) ON DELETE CASCADE,
    CHECK ((disc = 'disc') = (disc_number IS NOT NULL))
) STRICT;
