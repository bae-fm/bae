-- The library's people: profiles, ratings and the library's rating systems,
-- playlists and plays. Part of the schema coven applies as host migration 1.

-- Profiles retain the stable member id even after membership removal.
-- `avatar_id` is the profile picture's coven file (`coven_files`); NULL is
-- no picture.
CREATE TABLE people (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    private_circle TEXT NOT NULL,
    history_sharing TEXT NOT NULL CHECK (history_sharing IN ('off', 'private', 'household')),
    avatar_id TEXT
) STRICT;
-- A profile by its picture's file, which coven's pending records name.
CREATE INDEX people_avatar ON people(avatar_id);

-- Ratings identify one person's judgment of an album or one release's track.
CREATE TABLE ratings (
    id TEXT PRIMARY KEY NOT NULL,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    album_id TEXT REFERENCES albums(id) ON DELETE CASCADE,
    track_id TEXT REFERENCES tracks(id) ON DELETE CASCADE,
    stars INTEGER NOT NULL CHECK (stars BETWEEN 1 AND 5),
    CHECK ((album_id IS NULL) <> (track_id IS NULL)),
    UNIQUE (person_id, album_id),
    UNIQUE (person_id, track_id)
) STRICT;

-- An album's ratings read along the album.
CREATE INDEX ratings_album ON ratings(album_id);

-- How the library shows ratings, for albums and for tracks: as stars, as a
-- heart for three stars or more, or hidden. One row, keyed 'library', which
-- the library's first write inserts with stars for both, and which only an
-- admin changes.
CREATE TABLE rating_systems (
    id TEXT PRIMARY KEY NOT NULL CHECK (id = 'library'),
    albums TEXT NOT NULL CHECK (albums IN ('stars', 'heart', 'hidden')),
    tracks TEXT NOT NULL CHECK (tracks IN ('stars', 'heart', 'hidden'))
) STRICT;

CREATE TABLE playlists (
    id TEXT PRIMARY KEY NOT NULL,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    audience TEXT NOT NULL
) STRICT;

-- The position id is the entry's own id, so the same track can sit in a
-- playlist twice. `position` orders the entries: an append takes the next
-- number, and a reorder or an insert at a place renumbers them from 0.
CREATE TABLE playlist_entries (
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    position_id TEXT NOT NULL,
    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (playlist_id, position_id)
) STRICT;
-- A playlist's entries read in order, and its last position found for an
-- append, along the playlist's own run of this index.
CREATE INDEX playlist_entries_position ON playlist_entries(playlist_id, position);

-- One play. `played_on` is the calendar date the playing device's clock
-- showed where it was when the play began (YYYY-MM-DD), which the play's
-- write stores, so History groups plays by the day they were played there.
CREATE TABLE history (
    id TEXT PRIMARY KEY NOT NULL,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    played_on TEXT NOT NULL CHECK (played_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    played_ms INTEGER NOT NULL CHECK (played_ms > 0),
    audience TEXT NOT NULL
) STRICT;
-- History lists everyone's plays newest first, and finds each day's newest
-- play, along these indexes; one person's plays along their own runs.
CREATE INDEX history_order ON history(started_at, id);
CREATE INDEX history_day ON history(played_on, started_at, id);
CREATE INDEX history_person_order ON history(person_id, started_at, id);
CREATE INDEX history_person_day ON history(person_id, played_on, started_at, id);
