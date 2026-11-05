-- Which disc of the release the user said each CUE sheet is, or that it is
-- ignored.
CREATE TABLE IF NOT EXISTS import_candidate_sheet_disc (
    content_hash TEXT NOT NULL,
    sheet_id     TEXT NOT NULL,
    disc         TEXT NOT NULL CHECK (disc IN ('disc', 'ignored')),
    disc_number  INTEGER CHECK (disc_number IS NULL OR disc_number >= 1),
    PRIMARY KEY (content_hash, sheet_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state(content_hash) ON DELETE CASCADE,
    CHECK ((disc = 'disc') = (disc_number IS NOT NULL))
) STRICT;

-- Which audio file the user bound each FILE reference of a CUE sheet to.
CREATE TABLE IF NOT EXISTS import_candidate_sheet_reference (
    content_hash TEXT NOT NULL,
    sheet_id TEXT NOT NULL,
    file_reference TEXT NOT NULL,
    file_id TEXT,
    PRIMARY KEY (content_hash, sheet_id, file_reference),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state(content_hash) ON DELETE CASCADE
) STRICT;
