-- A receiver the cast picker lists: what discovery last found on the network,
-- or what a host's browser reported, merged into one list and written whole
-- in one write per change. Never synced. Discovery doesn't outlive the
-- process that ran it, so the table is emptied as the library opens.
CREATE TABLE IF NOT EXISTS cast_receiver (
    id   TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('cast', 'dlna', 'airplay')),
    -- What the receiver states of itself; NULL when it states no model.
    model TEXT,
    -- The host discovery found it at.
    address TEXT NOT NULL
) STRICT;
