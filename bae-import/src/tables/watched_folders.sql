-- The folders the desktop app watches for releases to import, in the user's
-- order, each path coven's bytes for it (`coven::path_bytes`). Scan results
-- belong to one watched folder (`folder_scan_roots` and the scans' tables).
-- What is known about a folder under one — the user's decisions, and when it
-- was found — is keyed by its path on disk, so it outlives a takeover and
-- goes when no watched folder covers the folder.
CREATE TABLE IF NOT EXISTS watched_import_folders (
    path      BLOB PRIMARY KEY,
    position  INTEGER NOT NULL UNIQUE CHECK (position >= 0)
) STRICT;
