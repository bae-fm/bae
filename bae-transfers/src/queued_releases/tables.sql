-- The queues this device works through one release at a time: downloads to
-- keep releases offline or to make them local, and exports and saves. Whether each is paused. Never
-- synced. The download queue and its pause are kept across restarts; the
-- export and save queue opens empty and running.
CREATE TABLE IF NOT EXISTS release_queues (
    queue  TEXT PRIMARY KEY CHECK (queue IN ('download', 'output')),
    paused INTEGER NOT NULL CHECK (paused IN (0, 1))
) STRICT;
INSERT OR IGNORE INTO release_queues (queue, paused) VALUES ('download', 0), ('output', 0);

-- A release queued to download, in the order queued: to keep offline (a pin),
-- or into a folder the person picked, to make it local. It names its release
-- by id alone: an entry whose release leaves the library stays listed, for it
-- to be cancelled. One entry at most downloads; how far it has got is not
-- here, since it moves with every byte.
CREATE TABLE IF NOT EXISTS download_queue (
    position   INTEGER PRIMARY KEY,
    release_id TEXT NOT NULL UNIQUE,
    -- When it was queued, in milliseconds since the epoch.
    created_at INTEGER NOT NULL,
    state      TEXT NOT NULL CHECK (state IN ('queued', 'active', 'failed')),
    error      TEXT,
    kind       TEXT NOT NULL CHECK (kind IN ('pin', 'move_to_this_device')),
    -- A move-to-this-device's folder, as coven's bytes for the path
    -- (`coven::path_bytes`); the release goes in a folder of its own below it.
    target_dir BLOB,
    CHECK ((state = 'failed') = (error IS NOT NULL)),
    CHECK ((kind = 'move_to_this_device') = (target_dir IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS download_queue_active
    ON download_queue (state) WHERE state = 'active';

-- The releases the download queue has held of each kind since that kind last
-- had none waiting or downloading: what the Activity panel's Pinning and
-- Moving to this device lines count. Only membership: whether a release is done is read
-- from release_storage. Never synced.
CREATE TABLE IF NOT EXISTS download_batch (
    release_id TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('pin', 'move_to_this_device')),
    PRIMARY KEY (release_id, kind)
) STRICT, WITHOUT ROWID;

-- How many batches of each kind the download queue has begun: the number of
-- the batch download_batch holds of that kind now. A reader holding a batch's
-- bytes tells by it that the batch ended and another began, whether or not it
-- read the kind with none in between. Never synced.
CREATE TABLE IF NOT EXISTS download_batch_number (
    kind   TEXT PRIMARY KEY CHECK (kind IN ('pin', 'move_to_this_device')),
    number INTEGER NOT NULL CHECK (number >= 0)
) STRICT, WITHOUT ROWID;

INSERT OR IGNORE INTO download_batch_number (kind, number) VALUES ('pin', 0), ('move_to_this_device', 0);

-- A kind's first release in its batch begins the next batch.
CREATE TRIGGER IF NOT EXISTS download_batch_begun AFTER INSERT ON download_batch
WHEN NOT EXISTS (
    SELECT 1 FROM download_batch WHERE kind = NEW.kind AND release_id <> NEW.release_id)
BEGIN
    UPDATE download_batch_number SET number = number + 1 WHERE kind = NEW.kind;
END;

-- An entry waiting or downloading is in its kind's batch: queued, retried, put
-- back by a pause, started, or queued again as the other kind in its place.
CREATE TRIGGER IF NOT EXISTS download_batch_queued AFTER INSERT ON download_queue
WHEN NEW.state IN ('queued', 'active') AND NOT EXISTS (
    SELECT 1 FROM download_batch WHERE release_id = NEW.release_id AND kind = NEW.kind)
BEGIN
    INSERT INTO download_batch (release_id, kind) VALUES (NEW.release_id, NEW.kind);
END;

CREATE TRIGGER IF NOT EXISTS download_batch_requeued AFTER UPDATE OF state, kind ON download_queue
WHEN NEW.state IN ('queued', 'active') AND NOT EXISTS (
    SELECT 1 FROM download_batch WHERE release_id = NEW.release_id AND kind = NEW.kind)
BEGIN
    INSERT INTO download_batch (release_id, kind) VALUES (NEW.release_id, NEW.kind);
END;

-- A kind's batch ends once none of its entries waits or downloads: its last
-- one finished, was cancelled, failed, or was queued again as the other kind.
-- The releases that left before then stay in the batch.
CREATE TRIGGER IF NOT EXISTS download_batch_left AFTER DELETE ON download_queue
BEGIN
    DELETE FROM download_batch WHERE kind = OLD.kind AND NOT EXISTS (
        SELECT 1 FROM download_queue WHERE kind = OLD.kind AND state IN ('queued', 'active'));
END;

CREATE TRIGGER IF NOT EXISTS download_batch_stopped AFTER UPDATE OF state, kind ON download_queue
WHEN NEW.state = 'failed' OR NEW.kind <> OLD.kind
BEGIN
    DELETE FROM download_batch WHERE kind = OLD.kind AND NOT EXISTS (
        SELECT 1 FROM download_queue WHERE kind = OLD.kind AND state IN ('queued', 'active'));
END;

-- A release queued to export or save into a folder, as `download_queue` holds
-- a download. How far it has got is not here either.
CREATE TABLE IF NOT EXISTS output_queue (
    position   INTEGER PRIMARY KEY,
    release_id TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    state      TEXT NOT NULL CHECK (state IN ('queued', 'active', 'failed')),
    error      TEXT,
    -- coven's bytes for the folder's path (`coven::path_bytes`).
    target_dir BLOB NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('export', 'save')),
    -- A save's preset, its filename and folder name patterns and its extras,
    -- captured
    -- whole as JSON when it was queued, so a later edit to any changes nothing
    -- already queued.
    preset          TEXT,
    filename_pattern TEXT,
    folder_pattern   TEXT,
    extras          TEXT,
    CHECK ((state = 'failed') = (error IS NOT NULL)),
    CHECK ((kind = 'save') = (preset IS NOT NULL)),
    CHECK ((kind = 'save') = (filename_pattern IS NOT NULL)),
    CHECK ((kind = 'save') = (folder_pattern IS NOT NULL)),
    CHECK ((kind = 'save') = (extras IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS output_queue_active
    ON output_queue (state) WHERE state = 'active';

