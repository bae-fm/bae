-- The library's people: profiles, ratings, playlists and plays. Part of the
-- schema coven applies as host migration 1.

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

CREATE TABLE playlists (
    id TEXT PRIMARY KEY NOT NULL,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    audience TEXT NOT NULL
) STRICT;

-- The position id distinguishes repeated tracks and concurrent insertions.
-- Integer order is not unique: ties retain both entries, ordered by position id.
CREATE TABLE playlist_entries (
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    position_id TEXT NOT NULL,
    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (playlist_id, position_id)
) STRICT;
-- A playlist's entries read in order, and its last position found for an
-- append, along the playlist's own run of this index.
CREATE INDEX playlist_entries_position ON playlist_entries(playlist_id, position, position_id);

CREATE TABLE history (
    id TEXT PRIMARY KEY NOT NULL,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    played_ms INTEGER NOT NULL CHECK (played_ms > 0),
    audience TEXT NOT NULL
) STRICT;
CREATE INDEX history_started_at ON history(started_at);

-- Before storage establishes a person, listening stays private on this device.
-- Creating the profile transfers these plays to its private circle atomically.
CREATE TABLE local_history (
    id TEXT PRIMARY KEY NOT NULL,
    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    played_ms INTEGER NOT NULL CHECK (played_ms > 0)
) STRICT;

-- The name the library's creator typed when they made it, kept until they are
-- a member (storage set up) and one write asks for their profile under it and
-- deletes the row, as coven keeps a joiner's name until a write acknowledges
-- it. It is an intent, not a profile: nothing reads it as one. At most one,
-- and only on the device that made the library. Never synced.
CREATE TABLE creator_name (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0)
) STRICT;

-- The profile this device's person asked for while their private circle is
-- being made: the operation coven journaled for the circle (as
-- `coven_operations` lists it), the circle it makes, and the name they gave.
-- The write that journals the circle inserts it. Once the circle's entry
-- lands, the write that creates the profile from it, and moves
-- `local_history` into the circle, deletes it; so does the write that
-- acknowledges a circle the library refused. A person asks for one profile,
-- so there is at most one. Never synced.
CREATE TABLE profile_request (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    operation INTEGER NOT NULL,
    circle TEXT NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0)
) STRICT;
