-- The releases with files queued to upload since none had any: what the
-- Activity panel's Uploading line counts. Only membership: whether a release
-- is done is read from release_storage. Kept by the triggers below in the
-- write that changes a release's queued files. Never synced.
CREATE TABLE IF NOT EXISTS upload_batch (
    release_id TEXT PRIMARY KEY
) STRICT, WITHOUT ROWID;

-- A release with a file queued to upload is in the batch: a file staged for
-- the cloud, or a release's storage row rewritten with files queued.
CREATE TRIGGER IF NOT EXISTS upload_batch_queued AFTER INSERT ON release_storage
WHEN NEW.queued_count > 0
    AND NOT EXISTS (SELECT 1 FROM upload_batch WHERE release_id = NEW.release_id)
BEGIN
    INSERT INTO upload_batch (release_id) VALUES (NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS upload_batch_requeued AFTER UPDATE OF queued_count ON release_storage
WHEN NEW.queued_count > 0
    AND NOT EXISTS (SELECT 1 FROM upload_batch WHERE release_id = NEW.release_id)
BEGIN
    INSERT INTO upload_batch (release_id) VALUES (NEW.release_id);
END;

-- The batch ends once none of its releases has a file queued: the last file
-- stored or no longer queued, or the last release with files queued removed
-- from the library. The releases that finished or left before then stay in
-- the batch.
CREATE TRIGGER IF NOT EXISTS upload_batch_unqueued AFTER UPDATE OF queued_count ON release_storage
WHEN NEW.queued_count = 0
BEGIN
    DELETE FROM upload_batch WHERE NOT EXISTS (
        SELECT 1 FROM upload_batch AS batch
        JOIN release_storage AS storage ON storage.release_id = batch.release_id
        WHERE storage.queued_count > 0);
END;

CREATE TRIGGER IF NOT EXISTS upload_batch_removed AFTER DELETE ON release_storage
BEGIN
    DELETE FROM upload_batch WHERE NOT EXISTS (
        SELECT 1 FROM upload_batch AS batch
        JOIN release_storage AS storage ON storage.release_id = batch.release_id
        WHERE storage.queued_count > 0);
END;
