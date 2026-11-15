-- What the person picked for the album a candidate files under, over the one
-- its records name. No row: the import files under the album the records name,
-- or a new one. A row with an album: that library album; a row with no album:
-- a new album. An album deleted while picked takes its pick with it.
CREATE TABLE IF NOT EXISTS import_candidate_album_pick (
    content_hash TEXT PRIMARY KEY,
    album_id     TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
) STRICT;
