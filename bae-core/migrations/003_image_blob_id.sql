-- Give a library image's blob an id of its own, distinct from the row's primary
-- key. coven's `(namespace, blob id)` names one immutable byte-string: the push
-- skips the upload when the object already exists, the pull skips the download
-- when the file is already cached, and a cache hit verifies only the byte length.
-- Replacing an image's bytes therefore has to mean a NEW blob id (with the old
-- blob deleted), never new bytes under the old id — which coven refuses outright
-- (`BlobAlreadyReferenced`).
--
-- `covers.id` / `artist_images.id` are the release / artist id, so they cannot
-- move. `blob_id` can: each stored image gets a fresh one, and changing a cover
-- repoints the row at a new blob and deletes the old.
--
-- Existing rows are backfilled `blob_id = id`, which is the blob they already
-- reference — no bytes move and nothing re-uploads.
--
-- The tables are rebuilt rather than extended with ALTER TABLE ADD COLUMN: a NOT
-- NULL column added by ALTER needs a default, and an empty-string blob id is a
-- representable-but-invalid state. NOT NULL makes "an image row that references
-- no blob" impossible instead. `blob_id` is appended LAST so the existing column
-- order stays a prefix — a changeset from a peer on the previous schema matches
-- columns positionally.

CREATE TABLE covers_new (
    -- The release id this cover belongs to (1:1).
    id TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    source_url TEXT,
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    hash TEXT,
    -- The id of the coven blob holding this cover's bytes. A new one per stored
    -- image, so replacing a cover is a new blob rather than new bytes under a
    -- live id.
    blob_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

INSERT INTO covers_new
    (id, content_type, file_size, width, height, source, source_url, cloud_path,
     _updated_at, created_at, hash, blob_id)
SELECT
    id, content_type, file_size, width, height, source, source_url, cloud_path,
    _updated_at, created_at, hash, id
FROM covers;

DROP TABLE covers;
ALTER TABLE covers_new RENAME TO covers;

CREATE TABLE artist_images_new (
    -- The artist id this image belongs to (1:1).
    id TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    source TEXT NOT NULL,
    source_url TEXT,
    cloud_path TEXT,
    _updated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    hash TEXT,
    blob_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES artists (id) ON DELETE CASCADE
) STRICT;

INSERT INTO artist_images_new
    (id, content_type, file_size, width, height, source, source_url, cloud_path,
     _updated_at, created_at, hash, blob_id)
SELECT
    id, content_type, file_size, width, height, source, source_url, cloud_path,
    _updated_at, created_at, hash, id
FROM artist_images;

DROP TABLE artist_images;
ALTER TABLE artist_images_new RENAME TO artist_images;
