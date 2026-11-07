-- How many times each transfer queue's rows have moved: an entry added,
-- removed or moved to another place. Triggers on each queue advance its
-- version in the write that moves its entries. A read of a queue starts a
-- window from an entry an earlier read found only while its version stands
-- where it stood for that read (§9.2).
CREATE TABLE IF NOT EXISTS release_queue_versions (
    source  TEXT PRIMARY KEY CHECK (source IN ('download_queue', 'output_queue')),
    version INTEGER NOT NULL
) STRICT;

INSERT OR IGNORE INTO release_queue_versions (source, version) VALUES
    ('download_queue', 0), ('output_queue', 0);

-- The queues this device works through one release at a time: downloads to
-- keep releases offline, and exports and saves. Whether each is paused. Never
-- synced. The download queue and its pause are kept across restarts; the
-- export and save queue opens empty and running.
CREATE TABLE IF NOT EXISTS release_queues (
    queue  TEXT PRIMARY KEY CHECK (queue IN ('download', 'output')),
    paused INTEGER NOT NULL CHECK (paused IN (0, 1))
) STRICT;
INSERT OR IGNORE INTO release_queues (queue, paused) VALUES ('download', 0), ('output', 0);

-- A release queued to download for offline, in the order queued. It names its
-- release by id alone: an entry whose release leaves the library stays listed,
-- for it to be cancelled. One entry at most downloads; how far it has got is
-- not here, since it moves with every byte.
CREATE TABLE IF NOT EXISTS download_queue (
    position   INTEGER PRIMARY KEY,
    release_id TEXT NOT NULL UNIQUE,
    -- When it was queued, in milliseconds since the epoch.
    created_at INTEGER NOT NULL,
    state      TEXT NOT NULL CHECK (state IN ('queued', 'active', 'failed')),
    error      TEXT,
    CHECK ((state = 'failed') = (error IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS download_queue_active
    ON download_queue (state) WHERE state = 'active';

CREATE TRIGGER IF NOT EXISTS download_queue_order_added AFTER INSERT ON download_queue
BEGIN
    UPDATE release_queue_versions SET version = version + 1 WHERE source = 'download_queue';
END;

CREATE TRIGGER IF NOT EXISTS download_queue_order_removed AFTER DELETE ON download_queue
BEGIN
    UPDATE release_queue_versions SET version = version + 1 WHERE source = 'download_queue';
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
    -- A save's preset, captured whole as JSON when it was queued, so a later
    -- edit to the preset changes nothing already queued.
    preset     TEXT,
    CHECK ((state = 'failed') = (error IS NOT NULL)),
    CHECK ((kind = 'save') = (preset IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS output_queue_active
    ON output_queue (state) WHERE state = 'active';

CREATE TRIGGER IF NOT EXISTS output_queue_order_added AFTER INSERT ON output_queue
BEGIN
    UPDATE release_queue_versions SET version = version + 1 WHERE source = 'output_queue';
END;

CREATE TRIGGER IF NOT EXISTS output_queue_order_removed AFTER DELETE ON output_queue
BEGIN
    UPDATE release_queue_versions SET version = version + 1 WHERE source = 'output_queue';
END;
