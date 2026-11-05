-- A receiver the cast picker lists: what discovery last found on the network,
-- or what a host's browser reported, merged into one list and written whole
-- in one write per change. Never synced. Discovery doesn't outlive the
-- process that ran it, so the table is emptied as the library opens.
CREATE TABLE IF NOT EXISTS cast_device (
    id   TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('cast', 'dlna', 'airplay'))
) STRICT;
