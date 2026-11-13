-- Release storage, part of the schema coven applies as host migration 1.

-- Where each release's files are on this device: their count and total
-- size; `stored`, 1 when it has files and every one names a coven file
-- `coven_files` records as stored; `pinned_count`, how many of them coven's
-- `coven_pins` lists as kept whole on this device; `queued_count` and
-- `first_queued_position`, how many of them coven's `coven_upload_queue`
-- lists and the place of the first (NULL with none); and `move_failed`, 1
-- while coven's `coven_moves` lists a file of it in a failed move. One row per release,
-- written by the triggers below in the write that changes its files, their
-- coven records, pins or queue: bae's own or coven's.
CREATE TABLE IF NOT EXISTS release_storage (
    release_id TEXT PRIMARY KEY REFERENCES releases (id) ON DELETE CASCADE,
    file_count INTEGER NOT NULL,
    total_size INTEGER NOT NULL,
    stored INTEGER NOT NULL,
    pinned_count INTEGER NOT NULL,
    queued_count INTEGER NOT NULL,
    first_queued_position INTEGER,
    move_failed INTEGER NOT NULL CHECK (move_failed IN (0, 1)),
    CHECK ((queued_count = 0) = (first_queued_position IS NULL))
) STRICT;

-- The releases with files waiting in this device's upload queue, in the order
-- their first files wait: what the import list's Done tab runs first (§3.2).
CREATE INDEX IF NOT EXISTS idx_release_storage_uploading ON release_storage (first_queued_position, release_id)
    WHERE first_queued_position IS NOT NULL;

-- Inserting a release id into `release_storage_refresh` rewrites that
-- release's row from its files and coven's records of them. The view holds no
-- rows; it names the statement its INSTEAD OF trigger runs.
CREATE VIEW IF NOT EXISTS release_storage_refresh AS SELECT NULL AS release_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS release_storage_refresh INSTEAD OF INSERT ON release_storage_refresh
BEGIN
    INSERT INTO release_storage (
        release_id, file_count, total_size, stored, pinned_count,
        queued_count, first_queued_position, move_failed)
    SELECT r.id,
           (SELECT COUNT(*) FROM release_files f WHERE f.release_id = r.id),
           (SELECT COALESCE(SUM(coven.size), 0) FROM release_files f
            JOIN coven_files coven ON coven.id = f.file
            WHERE f.release_id = r.id),
           EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = r.id)
             AND NOT EXISTS (SELECT 1 FROM release_files f
                             LEFT JOIN coven_files coven ON coven.id = f.file
                             WHERE f.release_id = r.id AND coven.hash IS NULL),
           (SELECT COUNT(*) FROM release_files f
            JOIN coven_pins pin ON pin.file = f.file
            WHERE f.release_id = r.id),
           (SELECT COUNT(*) FROM release_files f
            JOIN coven_upload_queue queued ON queued.file = f.file
            WHERE f.release_id = r.id),
           (SELECT MIN(queued.position) FROM release_files f
            JOIN coven_upload_queue queued ON queued.file = f.file
            WHERE f.release_id = r.id),
           EXISTS (SELECT 1 FROM release_files f
                   JOIN coven_moves moves ON moves.file = f.file
                   WHERE f.release_id = r.id AND moves.state = 'failed')
    FROM releases r
    WHERE r.id = NEW.release_id
    ON CONFLICT (release_id) DO UPDATE SET
        file_count = excluded.file_count,
        total_size = excluded.total_size,
        stored = excluded.stored,
        pinned_count = excluded.pinned_count,
        queued_count = excluded.queued_count,
        first_queued_position = excluded.first_queued_position,
        move_failed = excluded.move_failed;
END;

-- A release arrives with no files: its files name it, so none is written or
-- applied before it.
CREATE TRIGGER IF NOT EXISTS release_storage_release_added AFTER INSERT ON releases
BEGIN
    INSERT INTO release_storage (
        release_id, file_count, total_size, stored, pinned_count,
        queued_count, first_queued_position, move_failed)
    VALUES (NEW.id, 0, 0, 0, 0, 0, NULL, 0);
END;

CREATE TRIGGER IF NOT EXISTS release_storage_file_added AFTER INSERT ON release_files
BEGIN
    INSERT INTO release_storage_refresh (release_id) VALUES (NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS release_storage_file_changed AFTER UPDATE OF file ON release_files
BEGIN
    INSERT INTO release_storage_refresh (release_id) VALUES (NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS release_storage_file_removed AFTER DELETE ON release_files
BEGIN
    INSERT INTO release_storage_refresh (release_id) VALUES (OLD.release_id);
END;

-- A release file's coven file being recorded, switched to its stored bytes
-- or deleted refreshes its release: whether the release is stored follows
-- its files' records, which coven changes with no change to a row of bae's,
-- in the write completing an upload. A record no release file names moves nothing.
CREATE TRIGGER IF NOT EXISTS release_storage_coven_file_added AFTER INSERT ON coven_files
BEGIN
    INSERT INTO release_storage_refresh (release_id)
    SELECT release_id FROM release_files WHERE file = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS release_storage_coven_file_switched AFTER UPDATE OF coordinates ON coven_files
BEGIN
    INSERT INTO release_storage_refresh (release_id)
    SELECT release_id FROM release_files WHERE file = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS release_storage_coven_file_removed AFTER DELETE ON coven_files
BEGIN
    INSERT INTO release_storage_refresh (release_id)
    SELECT release_id FROM release_files WHERE file = OLD.id;
END;

-- A release file's pin moves its release's pinned count, in the transaction
-- that changes the pin: bae's own write, or coven's own as a file is pinned
-- or unpinned. A pin is listed
-- only once the whole file is on this device. A file no release file names
-- moves nothing.
CREATE TRIGGER IF NOT EXISTS release_storage_file_pinned AFTER INSERT ON coven_pins
BEGIN
    UPDATE release_storage SET pinned_count = pinned_count + 1
    WHERE release_id = (SELECT release_id FROM release_files WHERE file = NEW.file);
END;

CREATE TRIGGER IF NOT EXISTS release_storage_file_unpinned AFTER DELETE ON coven_pins
BEGIN
    UPDATE release_storage SET pinned_count = pinned_count - 1
    WHERE release_id = (SELECT release_id FROM release_files WHERE file = OLD.file);
END;

-- A release file joining or leaving this device's upload queue moves its
-- release's queued count and first place, in the transaction that changes
-- the queue: staging the file for the cloud, its move switching it to the
-- cloud, its upload retiring, or the row no longer naming it. A file no
-- release file names moves nothing.
CREATE TRIGGER IF NOT EXISTS release_storage_file_queued AFTER INSERT ON coven_upload_queue
BEGIN
    UPDATE release_storage SET queued_count = queued_count + 1,
        first_queued_position = MIN(COALESCE(first_queued_position, NEW.position), NEW.position)
    WHERE release_id = (SELECT release_id FROM release_files WHERE file = NEW.file);
END;

CREATE TRIGGER IF NOT EXISTS release_storage_file_unqueued AFTER DELETE ON coven_upload_queue
BEGIN
    UPDATE release_storage SET queued_count = queued_count - 1,
        first_queued_position = CASE WHEN first_queued_position = OLD.position
            THEN (SELECT MIN(queued.position) FROM release_files f
                  JOIN coven_upload_queue queued ON queued.file = f.file
                  WHERE f.release_id = release_storage.release_id)
            ELSE first_queued_position END
    WHERE release_id = (SELECT release_id FROM release_files WHERE file = OLD.file);
END;

-- A release file's move failing, or its list ending, moves its release's
-- `move_failed`, in the transaction that changes the move: coven lists a
-- failed move's files until the app cancels it. A file no release file names
-- moves nothing.
CREATE TRIGGER IF NOT EXISTS release_storage_file_moved AFTER INSERT ON coven_moves
BEGIN
    INSERT INTO release_storage_refresh (release_id)
    SELECT release_id FROM release_files WHERE file = NEW.file;
END;

CREATE TRIGGER IF NOT EXISTS release_storage_move_changed AFTER UPDATE OF state ON coven_moves
BEGIN
    INSERT INTO release_storage_refresh (release_id)
    SELECT release_id FROM release_files WHERE file = NEW.file;
END;

CREATE TRIGGER IF NOT EXISTS release_storage_move_ended AFTER DELETE ON coven_moves
BEGIN
    INSERT INTO release_storage_refresh (release_id)
    SELECT release_id FROM release_files WHERE file = OLD.file;
END;
