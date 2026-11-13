-- Who the source credits on a draft track in a role, as it stated the role
-- (composers), in its order.
CREATE TABLE IF NOT EXISTS import_candidate_track_role (
    content_hash          TEXT NOT NULL,
    track_id              TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL CHECK (name <> ''),
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    -- The name the catalog prints for the artist where it is another than the
    -- artist's own.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    source                TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    role                  TEXT,
    PRIMARY KEY (content_hash, track_id, position),
    FOREIGN KEY (content_hash, track_id)
        REFERENCES import_candidate_track (content_hash, track_id) ON DELETE CASCADE
) STRICT;

-- Who the source credits in a role on the release itself rather than on one
-- track (Discogs's release-level composers), in its order.
CREATE TABLE IF NOT EXISTS import_candidate_release_role (
    content_hash          TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL CHECK (name <> ''),
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    -- The name the catalog prints for the artist where it is another than the
    -- artist's own.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    source                TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    role                  TEXT,
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE
) STRICT;

-- Every MusicBrainz work the draft's tracks perform, and the works those are
-- parts of or hold as parts, once each, in the order the source reached them.
CREATE TABLE IF NOT EXISTS import_candidate_work (
    content_hash        TEXT NOT NULL,
    musicbrainz_work_id TEXT NOT NULL CHECK (musicbrainz_work_id <> ''),
    position            INTEGER NOT NULL CHECK (position >= 0),
    title               TEXT NOT NULL,
    disambiguation      TEXT,
    work_type           TEXT,
    PRIMARY KEY (content_hash, musicbrainz_work_id),
    UNIQUE (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE
) STRICT;

-- A draft work's composers, in the work's order.
CREATE TABLE IF NOT EXISTS import_candidate_work_composer (
    content_hash          TEXT NOT NULL,
    musicbrainz_work_id   TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL CHECK (name <> ''),
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    -- The name the catalog prints for the artist where it is another than the
    -- artist's own.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    PRIMARY KEY (content_hash, musicbrainz_work_id, position),
    FOREIGN KEY (content_hash, musicbrainz_work_id)
        REFERENCES import_candidate_work (content_hash, musicbrainz_work_id) ON DELETE CASCADE
) STRICT;

-- A draft work that is part of another, in the order the source relates them.
CREATE TABLE IF NOT EXISTS import_candidate_work_part (
    content_hash     TEXT NOT NULL,
    position         INTEGER NOT NULL CHECK (position >= 0),
    parent_work_id   TEXT NOT NULL,
    child_work_id    TEXT NOT NULL,
    PRIMARY KEY (content_hash, position),
    UNIQUE (content_hash, parent_work_id, child_work_id),
    FOREIGN KEY (content_hash, parent_work_id)
        REFERENCES import_candidate_work (content_hash, musicbrainz_work_id) ON DELETE CASCADE,
    FOREIGN KEY (content_hash, child_work_id)
        REFERENCES import_candidate_work (content_hash, musicbrainz_work_id) ON DELETE CASCADE
) STRICT;

-- The works a draft track performs, in the order it performs them.
CREATE TABLE IF NOT EXISTS import_candidate_track_work (
    content_hash        TEXT NOT NULL,
    track_id            TEXT NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    musicbrainz_work_id TEXT NOT NULL,
    PRIMARY KEY (content_hash, track_id, position),
    UNIQUE (content_hash, track_id, musicbrainz_work_id),
    FOREIGN KEY (content_hash, track_id)
        REFERENCES import_candidate_track (content_hash, track_id) ON DELETE CASCADE,
    FOREIGN KEY (content_hash, musicbrainz_work_id)
        REFERENCES import_candidate_work (content_hash, musicbrainz_work_id) ON DELETE CASCADE
) STRICT;
