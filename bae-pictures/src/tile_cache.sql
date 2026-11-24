-- The tile cache's table. Part of the schema coven applies as host
-- migration 1.

-- One row per folder in the tile cache's folder on this device: the tiles of
-- one picture a viewer zoomed, a library picture or an import picture. The
-- cache writes a folder before its row, and deletes a row before its folder,
-- so every row names a folder that is there. Settings → Storage adds `bytes`
-- to its Cache figure, and the cache removes the least recently used rows
-- first. Never synced.
CREATE TABLE IF NOT EXISTS cached_tiles (
    -- The folder's name: the SHA-256 of what names the picture's bytes.
    folder TEXT PRIMARY KEY NOT NULL,
    -- The picture's size upright, in pixels, which its levels follow from.
    width INTEGER NOT NULL CHECK (width > 0),
    height INTEGER NOT NULL CHECK (height > 0),
    -- How its tiles are encoded: PNG for a picture with transparency.
    format TEXT NOT NULL CHECK (format IN ('jpeg', 'png')),
    -- The bytes of the folder's files.
    bytes INTEGER NOT NULL CHECK (bytes >= 0),
    -- The folder's place in the order of the cache's cuts and opens, higher
    -- for a later one: the cut that keeps a folder and each viewer that opens
    -- it set it to one past the highest.
    used INTEGER NOT NULL UNIQUE
) STRICT;
