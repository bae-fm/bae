-- The library album the person picked for a candidate to join, over the one
-- its records name. No row: the import files under the album the records name,
-- or a new one. An album deleted while picked takes its pick with it.
CREATE TABLE IF NOT EXISTS import_candidate_album_pick (
    content_hash TEXT PRIMARY KEY,
    album_id     TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
) STRICT;
