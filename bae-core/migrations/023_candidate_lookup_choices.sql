-- What a person decided one candidate's identification asks about: which of
-- the extracted signals a run leaves out, and which of the extracted catalog
-- numbers it looks up.
--
-- Held here rather than inside a run, because the decision outlives the run it
-- was made during: every later run of the candidate reads the same value, and
-- changing it is what starts the next one. No row means nobody has decided
-- anything — the disc ID and the barcodes are asked about, and no catalog
-- number is.
CREATE TABLE import_candidate_lookup_choices (
    content_hash     TEXT PRIMARY KEY,
    disc_id_excluded INTEGER NOT NULL CHECK (disc_id_excluded IN (0, 1)),
    barcode_excluded INTEGER NOT NULL CHECK (barcode_excluded IN (0, 1)),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The chosen catalog numbers, in the order they were chosen: each is looked up
-- on its own, and the order is the order the lookups are dispatched and their
-- results laid out, so it is part of the value rather than a set.
CREATE TABLE import_candidate_chosen_catalog (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL,
    value        TEXT NOT NULL,
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_lookup_choices (content_hash) ON DELETE CASCADE
) STRICT;
