-- A mark is a name the object itself carries — a barcode read off a scan, a
-- catalog number read off the folder name, a disc ID derived from a rip log's
-- table of contents. Extraction already reads these off an import candidate
-- and stores them with the file and the box they were read at; until now they
-- died at commit. One row per reading: the same barcode read off two scans is
-- two rows, each naming its own surface.
--
-- What a catalog says the barcode is is not a mark. That lives in that
-- catalog's record.
CREATE TABLE release_marks (
    id            TEXT NOT NULL PRIMARY KEY,
    release_id    TEXT NOT NULL,
    -- Where this reading sits in the order extraction read them, so a surface
    -- that names the surfaces a value was read from names them in that order.
    position      INTEGER NOT NULL,
    kind          TEXT NOT NULL,
    value         TEXT NOT NULL CHECK (value <> ''),
    origin        TEXT NOT NULL,
    -- The candidate-relative path of the file it was read off, or NULL where
    -- the surface is not a file (the folder's own name).
    origin_path   TEXT,
    -- The box the detector drew around the value, as fractions of the image.
    -- All four or none: half a box crops nothing.
    region_x      REAL,
    region_y      REAL,
    region_width  REAL,
    region_height REAL,
    _updated_at   TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    CHECK (
        (region_x IS NULL AND region_y IS NULL
         AND region_width IS NULL AND region_height IS NULL)
        OR (region_x IS NOT NULL AND region_y IS NOT NULL
            AND region_width IS NOT NULL AND region_height IS NOT NULL)
    ),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX idx_release_marks_release ON release_marks (release_id);

-- The disc ID column held what a re-identify pass recomputed from the stored
-- tracks, and nothing ever read it back. The mark is the one place a disc ID
-- lives now, so the column goes rather than standing beside it saying the same
-- thing from a different reading.
ALTER TABLE releases DROP COLUMN disc_id;
