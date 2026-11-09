-- The artist list counts each artist's albums (§2.3). `artist_listing` holds what
-- it sorts and the grid grouped by artist groups by, kept by the triggers below
-- and by `album_artist_link`'s (`album/listing.sql`).

-- Each artist the artist list shows: one an album the library shows credits,
-- with its name for the list's order, its sort name (else name) for the grid
-- grouped by artist, and how many of those albums credit it.
CREATE TABLE IF NOT EXISTS artist_listing (
    artist_id TEXT PRIMARY KEY REFERENCES artists (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    sort_key TEXT NOT NULL,
    album_count INTEGER NOT NULL CHECK (album_count > 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artist_listing_name ON artist_listing (name, artist_id);

CREATE INDEX IF NOT EXISTS idx_artist_listing_name_desc ON artist_listing (name DESC, artist_id);

CREATE INDEX IF NOT EXISTS idx_artist_listing_album_count ON artist_listing (album_count, name, artist_id);

CREATE INDEX IF NOT EXISTS idx_artist_listing_album_count_desc ON artist_listing (album_count DESC, name, artist_id);

-- Both columns run one way, so the index serves the grid grouped by artist in
-- either direction. It is unique, as the artist id makes it, so SQLite knows
-- each of its entries is one artist and reads each artist's albums from
-- `album_artist_link`'s index in the grid's sort instead of sorting them.
CREATE UNIQUE INDEX IF NOT EXISTS idx_artist_listing_sort_key ON artist_listing (sort_key COLLATE NOCASE DESC, artist_id DESC);

-- Each link added to or removed from `album_artist_link` counts in or out of
-- its artist's listing row. An artist whose own row has not arrived yet gets
-- its listing row when it does (`artist_listing_added`).
CREATE TRIGGER IF NOT EXISTS album_artist_link_added AFTER INSERT ON album_artist_link
BEGIN
    INSERT INTO artist_listing (artist_id, name, sort_key, album_count)
    SELECT id, name, COALESCE(sort_name, name), 1 FROM artists WHERE id = NEW.artist_id
    ON CONFLICT (artist_id) DO UPDATE SET album_count = album_count + 1;
END;

CREATE TRIGGER IF NOT EXISTS album_artist_link_removed AFTER DELETE ON album_artist_link
BEGIN
    DELETE FROM artist_listing WHERE artist_id = OLD.artist_id AND album_count = 1;
    UPDATE artist_listing SET album_count = album_count - 1 WHERE artist_id = OLD.artist_id;
END;

-- An artist arriving takes a listing row for the links already counted under
-- it, however many of its own links have counted in by then.
CREATE TRIGGER IF NOT EXISTS artist_listing_added AFTER INSERT ON artists
WHEN EXISTS (SELECT 1 FROM album_artist_link WHERE artist_id = NEW.id)
BEGIN
    INSERT INTO artist_listing (artist_id, name, sort_key, album_count)
    VALUES (NEW.id, NEW.name, COALESCE(NEW.sort_name, NEW.name),
            (SELECT COUNT(*) FROM album_artist_link WHERE artist_id = NEW.id))
    ON CONFLICT (artist_id) DO UPDATE SET album_count = excluded.album_count;
END;

-- A renamed artist moves in the artist list.
CREATE TRIGGER IF NOT EXISTS artist_listing_renamed AFTER UPDATE OF name, sort_name ON artists
BEGIN
    UPDATE artist_listing SET name = NEW.name, sort_key = COALESCE(NEW.sort_name, NEW.name)
    WHERE artist_id = NEW.id;
END;
