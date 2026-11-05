-- What this device plays next, and how. One row; never synced. Up Next is
-- `playback_manual`, the context lane `playback_context`.
CREATE TABLE IF NOT EXISTS playback_queue (
    singleton  INTEGER PRIMARY KEY CHECK (singleton = 1),
    repeat     TEXT NOT NULL CHECK (repeat IN ('off', 'track', 'context')),
    -- The entry number the next entry of either lane gets: no two entries
    -- ever share one, so an id a screen still holds never names a later row.
    next_entry INTEGER NOT NULL,
    -- What the context lane plays from (one release, several, a list of
    -- tracks, or the library), whether it plays shuffled, and the position of
    -- its track playing or last played: the lane's upcoming tracks are the
    -- ones after it. NULL together exactly while the lane is empty. Which
    -- releases several were, or which tracks a list named, is the lane's
    -- tracks.
    source_kind       TEXT CHECK (source_kind IN ('release', 'releases', 'tracks', 'library')),
    -- The release the lane plays, set exactly when it plays one.
    source_release_id TEXT,
    shuffled          INTEGER CHECK (shuffled IN (0, 1)),
    cursor            INTEGER,
    -- How many times tracks went into Up Next since playback started with
    -- the library, and how many tracks the last of those put in, NULL before
    -- the first. The write that adds the tracks counts them here, so a
    -- screen confirming each addition reads one value per addition, even
    -- two alike in a row. Starting playback with the library zeroes both.
    additions    INTEGER NOT NULL CHECK (additions >= 0),
    added_tracks INTEGER CHECK (added_tracks > 0),
    -- How many writes have put entries into Up Next, taken them out or moved
    -- them; and how many have changed the context lane's tracks after its
    -- playing one: put them in, taken them out, reordered them, or moved
    -- which track is playing. Each write that does so counts it here, so a
    -- queue screen holding a row it moved knows when the lane's read shows
    -- the move, and its rows know when to read again.
    manual_version  INTEGER NOT NULL CHECK (manual_version >= 0),
    context_version INTEGER NOT NULL CHECK (context_version >= 0),
    CHECK ((source_kind IS NULL) = (shuffled IS NULL)
        AND (source_kind IS NULL) = (cursor IS NULL)),
    CHECK ((source_kind IS 'release') = (source_release_id IS NOT NULL)),
    CHECK ((additions = 0) = (added_tracks IS NULL))
) STRICT;

INSERT OR IGNORE INTO playback_queue (
    singleton, repeat, next_entry, additions, manual_version, context_version)
VALUES (1, 'off', 1, 0, 0, 0);

-- Up Next: tracks queued by hand, drained first, in position order. Each is an
-- entry of its own, so a track queued twice is two rows. Positions are only an
-- order: the lane has gaps where entries left it. Never synced.
CREATE TABLE IF NOT EXISTS playback_manual (
    entry    INTEGER PRIMARY KEY,
    position INTEGER NOT NULL UNIQUE,
    track_id TEXT NOT NULL
) STRICT;
-- A deleted track's entries are found by its id.
CREATE INDEX IF NOT EXISTS playback_manual_track ON playback_manual(track_id);

-- The context lane a play fills, in position order. Each row is an entry of
-- its own, as in Up Next, so a track named twice is two rows. A play numbers
-- its entries in track order and puts them in in that order, so the table and
-- its track index grow at their end and only the position index takes rows
-- out of order. Positions are only an order, with gaps where rows left. No
-- trigger runs on its rows: a play takes out and puts in up to the whole
-- library's, so a trigger would run once for each, and SQLite empties a table
-- with a trigger row by row. Each write that leaves the lane empty clears
-- what it played from itself. Never synced.
CREATE TABLE IF NOT EXISTS playback_context (
    entry      INTEGER PRIMARY KEY,
    position   INTEGER NOT NULL UNIQUE,
    track_id   TEXT NOT NULL,
    -- The row's position in the order unshuffling puts the lane back in: its
    -- order when shuffle turned on, or the source's order for a shuffled play.
    -- Read only while the lane is shuffled.
    unshuffled INTEGER NOT NULL
) STRICT;
-- A deleted track's rows are found by its id.
CREATE INDEX IF NOT EXISTS playback_context_track ON playback_context(track_id);

-- A deleted track leaves the queue in the same transaction. A trigger rather
-- than a foreign key: the queue's rows are only ever made from tracks the
-- library holds, so a key would only look each one up again as a play fills
-- the lane.
CREATE TRIGGER IF NOT EXISTS tracks_leave_playback_queue AFTER DELETE ON tracks
BEGIN
    UPDATE playback_queue SET manual_version = manual_version + 1
    WHERE EXISTS (SELECT 1 FROM playback_manual WHERE track_id = OLD.id);
    UPDATE playback_queue SET context_version = context_version + 1
    WHERE EXISTS (SELECT 1 FROM playback_context WHERE track_id = OLD.id);
    DELETE FROM playback_manual WHERE track_id = OLD.id;
    DELETE FROM playback_context WHERE track_id = OLD.id;
    -- The lane's last track deleted takes what it played from with it.
    UPDATE playback_queue
    SET source_kind = NULL, source_release_id = NULL, shuffled = NULL, cursor = NULL
    WHERE source_kind IS NOT NULL AND NOT EXISTS (SELECT 1 FROM playback_context);
END;

-- Where this device's playback stood, so a restart resumes it: the queue
-- entry playing and how far into its stream. The volume it resumes at is
-- `playback_now`'s. Present while a track is loaded; never synced.
CREATE TABLE IF NOT EXISTS playback_state (
    id               TEXT PRIMARY KEY,
    -- The entry id the queue screens show ('m' and an Up Next entry number,
    -- or 'c' and a context track). Read only with its track: a deleted track
    -- leaves nothing to resume.
    current_entry    TEXT,
    current_track_id TEXT REFERENCES tracks(id) ON DELETE SET NULL,
    position_ms      INTEGER CHECK (position_ms >= 0)
) STRICT;
