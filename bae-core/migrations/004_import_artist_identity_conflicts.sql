CREATE TABLE import_candidate_artist_identity_conflict (
    content_hash                       TEXT PRIMARY KEY,
    incoming_artist_name               TEXT NOT NULL,
    discogs_artist_id                  TEXT NOT NULL,
    musicbrainz_artist_id              TEXT NOT NULL,
    discogs_library_artist_id           TEXT NOT NULL,
    musicbrainz_library_artist_id       TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_failure (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (discogs_library_artist_id) REFERENCES artists (id) ON DELETE RESTRICT,
    FOREIGN KEY (musicbrainz_library_artist_id) REFERENCES artists (id) ON DELETE RESTRICT
) STRICT;
