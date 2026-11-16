-- Where each watched folder's scan stands, as the import list's chrome shows
-- it: the outcome its scan stored, or what the folder scan coordinator holds
-- of it that no scan stores, and which problems the app has raised an alert
-- for. Nothing in the two tables here outlives the process that wrote it, so
-- the import service empties them as the library opens.

-- What the folder scan coordinator holds of a folder's scan, written in the
-- step that changes it. 'running' while a whole-folder pass reads the folder,
-- is owed once the pass running ends, or a removal holds the folder. 'failed'
-- when the folder's last pass failed and its scan could not store the
-- failure, with that failure and the volume the pass found the folder on; the
-- folder's next pass deletes it. A removal holds the folder it watches in
-- place of the ones it removes before that folder is watched, so a row's
-- folder need not be watched yet.
CREATE TABLE IF NOT EXISTS folder_scan_held (
    watched_folder_path BLOB PRIMARY KEY,
    state               TEXT NOT NULL CHECK (state IN ('running', 'failed')),
    error               TEXT,
    volume              TEXT CHECK (volume IN ('local', 'network')),
    CHECK ((state = 'failed') = (error IS NOT NULL)),
    CHECK ((state = 'failed') = (volume IS NOT NULL))
) STRICT;

-- The problem of each watched folder that the app has raised an alert for:
-- its kind and its detail. The app raises an alert for each problem with no
-- row here and then writes one. When the folder's problem ends, the triggers
-- below delete the row, so the next problem raises its alert again, even with
-- the same detail.
CREATE TABLE IF NOT EXISTS folder_scan_alert_shown (
    watched_folder_path BLOB PRIMARY KEY,
    kind                TEXT NOT NULL CHECK (kind IN ('failed', 'unwatched')),
    detail              TEXT NOT NULL,
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path)
        ON DELETE CASCADE
) STRICT;

-- A new scan of the folder, or the end of one, ends the problem its last
-- outcome stored.
CREATE TRIGGER IF NOT EXISTS folder_scan_alert_shown_on_outcome
AFTER UPDATE OF status ON folder_scan_roots
WHEN NEW.status IS NOT OLD.status
BEGIN
    DELETE FROM folder_scan_alert_shown WHERE watched_folder_path = NEW.watched_folder_path;
END;

-- A failure the coordinator held ends when its row goes or changes.
CREATE TRIGGER IF NOT EXISTS folder_scan_alert_shown_on_held_removed
AFTER DELETE ON folder_scan_held
WHEN OLD.state = 'failed'
BEGIN
    DELETE FROM folder_scan_alert_shown WHERE watched_folder_path = OLD.watched_folder_path;
END;

CREATE TRIGGER IF NOT EXISTS folder_scan_alert_shown_on_held_changed
AFTER UPDATE ON folder_scan_held
WHEN OLD.state = 'failed'
BEGIN
    DELETE FROM folder_scan_alert_shown WHERE watched_folder_path = OLD.watched_folder_path;
END;

-- Each watched folder's scan status and its detail, with its place in the
-- list and the volume it is shown on:
--
-- - A failure the coordinator holds is the status, over anything stored.
-- - A folder no scan has opened a generation for is 'not_scanned'.
-- - An outcome the current generation stored is the status.
-- - A generation with no outcome is 'scanning' while the coordinator holds
--   the folder as running. Otherwise the pass that opened it ended without
--   storing what became of it, as one that panicked does, and the status is
--   'failed' with that as its detail.
CREATE VIEW IF NOT EXISTS folder_scan_status AS
SELECT
    watched.path     AS watched_folder_path,
    watched.position AS position,
    CASE
        WHEN held.state = 'failed' THEN 'failed'
        WHEN roots.watched_folder_path IS NULL THEN 'not_scanned'
        WHEN roots.status IS NOT NULL THEN roots.status
        WHEN held.state = 'running' THEN 'scanning'
        ELSE 'failed'
    END AS status,
    CASE
        WHEN held.state = 'failed' THEN held.error
        WHEN roots.watched_folder_path IS NULL THEN NULL
        WHEN roots.status IS NOT NULL THEN roots.detail
        WHEN held.state = 'running' THEN NULL
        ELSE 'the scan stopped before its result could be stored'
    END AS detail,
    (CASE WHEN held.state = 'failed' THEN held.volume ELSE roots.volume END) IS 'network'
        AS on_network_volume
FROM watched_import_folders AS watched
LEFT JOIN folder_scan_roots AS roots ON roots.watched_folder_path = watched.path
LEFT JOIN folder_scan_held AS held ON held.watched_folder_path = watched.path;
