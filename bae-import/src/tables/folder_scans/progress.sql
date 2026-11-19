-- Each watched folder's scan status, as the import list's chrome shows it:
-- the outcome its scan stored, or the state the folder scan coordinator keeps
-- for it that no scan stores. The table here describes only the process that
-- wrote it, so the import service empties it as the library opens.

-- The state the folder scan coordinator keeps for a folder's scan, which the
-- coordinator writes in the step that changes it. The state is 'running'
-- while a whole-folder pass reads the folder, while the coordinator will
-- start one once the running pass ends, or while a removal holds the folder.
-- The state is 'failed' when the folder's last pass failed and its scan could
-- not store the failure, with that failure and the volume the pass found the
-- folder on; the folder's next pass deletes the row. A removal holds the
-- folder it watches in place of the ones it removes before the coordinator
-- watches that folder, so a row's folder need not be watched yet.
CREATE TABLE IF NOT EXISTS folder_scan_held (
    watched_folder_path BLOB PRIMARY KEY,
    state               TEXT NOT NULL CHECK (state IN ('running', 'failed')),
    error               TEXT,
    volume              TEXT CHECK (volume IN ('local', 'network')),
    CHECK ((state = 'failed') = (error IS NOT NULL)),
    CHECK ((state = 'failed') = (volume IS NOT NULL))
) STRICT;

-- Each watched folder's scan status and its detail, with its place in the
-- list and the volume it is shown on, which is NULL for a folder no scan has
-- opened a generation for:
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
        ELSE 'the scan stopped before bae stored its result'
    END AS detail,
    CASE
        WHEN held.state = 'failed' THEN NULL
        WHEN roots.status IS 'unwatched' THEN roots.unwatched_directory
    END AS unwatched_directory,
    CASE
        WHEN held.state = 'failed' THEN NULL
        WHEN roots.status IS 'unwatched' THEN roots.unwatched_cause
    END AS unwatched_cause,
    CASE WHEN held.state = 'failed' THEN held.volume ELSE roots.volume END AS volume
FROM watched_import_folders AS watched
LEFT JOIN folder_scan_roots AS roots ON roots.watched_folder_path = watched.path
LEFT JOIN folder_scan_held AS held ON held.watched_folder_path = watched.path;
