-- Which side of which medium of the release the user put each CUE sheet's
-- entries on, or that the sheet is ignored.
CREATE TABLE IF NOT EXISTS import_candidate_sheet_place (
    content_hash TEXT NOT NULL,
    sheet_id     TEXT NOT NULL,
    place        TEXT NOT NULL CHECK (place IN ('side', 'ignored')),
    medium       INTEGER CHECK (medium IS NULL OR medium >= 1),
    side         INTEGER CHECK (side IS NULL OR side >= 1),
    PRIMARY KEY (content_hash, sheet_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state(content_hash) ON DELETE CASCADE,
    CHECK ((place = 'side') = (medium IS NOT NULL)),
    CHECK ((place = 'side') = (side IS NOT NULL))
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
