-- The folder hash (SHA-256 over sorted relative paths, sizes and modification
-- times) of the files this device imported each release from. Another device's
-- folders mean nothing here, so it never syncs. A folder whose hash is here is
-- Done in the import list wherever it sits now, and one hash is one release on
-- this device.
CREATE TABLE IF NOT EXISTS local_release_origin (
    release_id   TEXT NOT NULL PRIMARY KEY REFERENCES releases(id) ON DELETE CASCADE,
    content_hash TEXT NOT NULL UNIQUE
) STRICT;
