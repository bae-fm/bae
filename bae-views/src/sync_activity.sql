-- How many files wait in this device's upload queue, for the sync activity:
-- one row, kept by the triggers below in the write that changes coven's
-- `coven_upload_queue`, so the activity reads one row, not the queue.
CREATE TABLE IF NOT EXISTS upload_queue_count (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    queued INTEGER NOT NULL CHECK (queued >= 0)
) STRICT;

INSERT OR IGNORE INTO upload_queue_count (singleton, queued) VALUES (1, 0);

-- A file joining or leaving the queue moves the count by one, as the rows of
-- `coven_upload_queue` do: staging the file for the cloud, its move switching
-- it, its upload retiring, or the row no longer naming it.
CREATE TRIGGER IF NOT EXISTS upload_queue_count_queued AFTER INSERT ON coven_upload_queue
BEGIN
    UPDATE upload_queue_count SET queued = queued + 1;
END;

CREATE TRIGGER IF NOT EXISTS upload_queue_count_unqueued AFTER DELETE ON coven_upload_queue
BEGIN
    UPDATE upload_queue_count SET queued = queued - 1;
END;
