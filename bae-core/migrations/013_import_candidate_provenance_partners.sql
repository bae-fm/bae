-- Find online pairs a MusicBrainz release and a Discogs release into one
-- pressing row when they agree on a barcode or a catalog number. Picking that
-- row claims both, and the candidate's three provenance columns can only name
-- the one the draft is read from. One row per other source the pick carries.
--
-- Keyed by source, not by position: a pressing lists a release once per source,
-- so a second row for the same source would be a second claim about the same
-- thing.
CREATE TABLE IF NOT EXISTS import_candidate_provenance_partner (
    content_hash TEXT NOT NULL,
    source       TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id   TEXT NOT NULL,
    PRIMARY KEY (content_hash, source),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash)
        ON DELETE CASCADE
) STRICT;
