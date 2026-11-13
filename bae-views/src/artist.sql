-- The artist list counts each artist's albums (§2.3). `artist_listing` holds what
-- it sorts and the grid grouped by artist groups by, kept by the triggers below
-- and by `album_artist_link`'s (`album/listing.sql`).

-- Each artist the artist list shows: one an album the library shows credits,
-- with its name and that name's sort key (`name_sort`) for the list's order,
-- the sort key of its sort name (else name) for the grid grouped by artist,
-- and how many of those albums credit it.
CREATE TABLE IF NOT EXISTS artist_listing (
    artist_id TEXT PRIMARY KEY REFERENCES artists (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    name_sort_key BLOB NOT NULL,
    sort_name_sort_key BLOB NOT NULL,
    album_count INTEGER NOT NULL CHECK (album_count > 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artist_listing_name ON artist_listing (name_sort_key, artist_id);

CREATE INDEX IF NOT EXISTS idx_artist_listing_name_desc ON artist_listing (name_sort_key DESC, artist_id);

CREATE INDEX IF NOT EXISTS idx_artist_listing_album_count ON artist_listing (album_count, name_sort_key, artist_id);

CREATE INDEX IF NOT EXISTS idx_artist_listing_album_count_desc ON artist_listing (album_count DESC, name_sort_key, artist_id);

-- Both columns run one way, so the index serves the grid grouped by artist in
-- either direction. It is unique, as the artist id makes it, so SQLite knows
-- each of its entries is one artist and reads each artist's albums from
-- `album_artist_link`'s index in the grid's sort instead of sorting them.
CREATE UNIQUE INDEX IF NOT EXISTS idx_artist_listing_sort_name_key ON artist_listing (sort_name_sort_key DESC, artist_id DESC);

-- Each link added to or removed from `album_artist_link` counts in or out of
-- its artist's listing row.
CREATE TRIGGER IF NOT EXISTS album_artist_link_added AFTER INSERT ON album_artist_link
BEGIN
    INSERT INTO artist_listing (artist_id, name, name_sort_key, sort_name_sort_key, album_count)
    SELECT id, name, bae_name_key(name, language), bae_name_key(COALESCE(sort_name, name), language), 1
    FROM artists CROSS JOIN (SELECT language FROM name_sort) WHERE id = NEW.artist_id
    ON CONFLICT (artist_id) DO UPDATE SET album_count = album_count + 1;
END;

CREATE TRIGGER IF NOT EXISTS album_artist_link_removed AFTER DELETE ON album_artist_link
BEGIN
    DELETE FROM artist_listing WHERE artist_id = OLD.artist_id AND album_count = 1;
    UPDATE artist_listing SET album_count = album_count - 1 WHERE artist_id = OLD.artist_id;
END;

-- A renamed artist moves in the artist list.
CREATE TRIGGER IF NOT EXISTS artist_listing_renamed AFTER UPDATE OF name, sort_name ON artists
BEGIN
    UPDATE artist_listing SET name = NEW.name,
        name_sort_key = bae_name_key(NEW.name, (SELECT language FROM name_sort)),
        sort_name_sort_key = bae_name_key(COALESCE(NEW.sort_name, NEW.name), (SELECT language FROM name_sort))
    WHERE artist_id = NEW.id;
END;
