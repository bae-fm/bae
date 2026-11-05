-- Where this device's playback is, for the surfaces that move the position on
-- themselves (the operating system's media controls, Media3): the playing
-- track's line and the preview's, each a position and whether it moves on
-- from there at the play rate. One row, written only as playback leaves a
-- line it holds: a seek, another track or clip, a pause or resume, a new
-- length, the pregap ending, or a position more than a second off the line.
-- Where playback is between those moves with the audio and is not here.
-- Starting playback with the library resets it to no line. Never synced.
CREATE TABLE IF NOT EXISTS playback_timeline (
    singleton   INTEGER PRIMARY KEY CHECK (singleton = 1),
    -- The track's line, on a timeline with no pregap: through the pregap it
    -- stands at the track's start. Set together once the playing track
    -- reports a position; the length NULL also while it is unknown.
    track_id    TEXT,
    position_ms INTEGER CHECK (position_ms >= 0),
    duration_ms INTEGER CHECK (duration_ms >= 0),
    moving      INTEGER CHECK (moving IN (0, 1)),
    -- How many seeks playback has applied since it started, so a surface
    -- that shows a seek ahead of it knows when it is applied.
    seek_revision INTEGER NOT NULL CHECK (seek_revision >= 0),
    -- The preview clip's line: the file range it auditions (its path as
    -- coven's bytes, the end NULL for the rest of the file), where it is, its
    -- length, and whether it moves on. Set together while a preview is
    -- loaded.
    preview_path         BLOB,
    preview_start_sample INTEGER CHECK (preview_start_sample >= 0),
    preview_end_sample   INTEGER CHECK (preview_end_sample >= 0),
    preview_position_ms  INTEGER CHECK (preview_position_ms >= 0),
    preview_duration_ms  INTEGER CHECK (preview_duration_ms >= 0),
    preview_moving       INTEGER CHECK (preview_moving IN (0, 1)),
    CHECK ((track_id IS NULL) = (position_ms IS NULL)
        AND (track_id IS NULL) = (moving IS NULL)),
    CHECK (duration_ms IS NULL OR track_id IS NOT NULL),
    CHECK ((preview_path IS NULL) = (preview_start_sample IS NULL)
        AND (preview_path IS NULL) = (preview_position_ms IS NULL)
        AND (preview_path IS NULL) = (preview_duration_ms IS NULL)
        AND (preview_path IS NULL) = (preview_moving IS NULL)),
    CHECK (preview_end_sample IS NULL OR preview_path IS NOT NULL)
) STRICT;

INSERT OR IGNORE INTO playback_timeline (singleton, seek_revision) VALUES (1, 0);
