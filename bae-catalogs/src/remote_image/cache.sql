-- The catalog image cache's table. Part of the schema coven applies as host
-- migration 1.

-- One row per file in the catalog image cache's folder on this device: the
-- answer the cache keeps for one image address, an image or that the address
-- serves none. The cache writes a file before its row, and deletes a row
-- before its file, so every row names a file that is there. Settings →
-- Storage adds `bytes` to its Cache figure, and the cache evicts the least
-- recently used rows first. Never synced.
CREATE TABLE IF NOT EXISTS cached_image (
    -- The file's name in the folder: the SHA-256 of the image's address.
    file TEXT PRIMARY KEY NOT NULL,
    -- The file's size in bytes.
    bytes INTEGER NOT NULL CHECK (bytes >= 0),
    -- The file's place in the order of the cache's reads and writes, higher
    -- for a later one: the write that keeps a file and each read that finds
    -- it set it to one past the highest.
    used INTEGER NOT NULL UNIQUE
) STRICT;
