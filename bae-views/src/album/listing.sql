-- The album grid (§2.1) lists each album holding a release, and grouped by
-- artist lists it once under each artist its credits show. These tables hold
-- what it sorts and groups by, kept by the triggers below.

-- Each album the library shows, with the values its sorts compare. `title_sort_key`
-- is the title's sort key in the device's language (`name_sort`). The credit
-- keys sort it by artist: `credit_sort_key` is the key of every credit's shown
-- artist's sort name (else name), each in credit order, so "A" sorts before
-- "A & B" and "A & B" before "A & C", and `credit_artist_ids` their ids,
-- joined by a separator below every printable character; both NULL for an
-- album with no credits.
CREATE TABLE IF NOT EXISTS album_listing (
    album_id TEXT PRIMARY KEY REFERENCES albums (id) ON DELETE CASCADE,
    title_sort_key BLOB NOT NULL,
    year INTEGER,
    created_at TEXT NOT NULL,
    credit_sort_key BLOB,
    credit_artist_ids TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS idx_album_listing_title ON album_listing (title_sort_key, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_title_desc ON album_listing (title_sort_key DESC, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_added ON album_listing (created_at, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_added_desc ON album_listing (created_at DESC, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_year ON album_listing (year IS NULL, year, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_year_desc ON album_listing (year IS NULL DESC, year DESC, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_credits ON album_listing (credit_sort_key, credit_artist_ids, album_id);

CREATE INDEX IF NOT EXISTS idx_album_listing_credits_desc ON album_listing (credit_sort_key DESC, credit_artist_ids DESC, album_id);

-- Each album the library shows and each artist one of its credits shows,
-- once however many credits name that artist, with the title key, year and date
-- added of the album's `album_listing` row: the grid grouped by artist reads
-- each artist's albums in the grid's sort from an index here, after
-- `artist_listing`'s index has put the artists in order, and sorts none.
CREATE TABLE IF NOT EXISTS album_artist_link (
    artist_id TEXT NOT NULL,
    album_id TEXT NOT NULL REFERENCES album_listing (album_id) ON DELETE CASCADE,
    title_sort_key BLOB NOT NULL,
    year INTEGER,
    created_at TEXT NOT NULL,
    PRIMARY KEY (artist_id, album_id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_album_artist_link_album ON album_artist_link (album_id);

CREATE INDEX IF NOT EXISTS idx_album_artist_link_title ON album_artist_link (artist_id, title_sort_key, album_id);

CREATE INDEX IF NOT EXISTS idx_album_artist_link_title_desc ON album_artist_link (artist_id, title_sort_key DESC, album_id);

CREATE INDEX IF NOT EXISTS idx_album_artist_link_added ON album_artist_link (artist_id, created_at, album_id);

CREATE INDEX IF NOT EXISTS idx_album_artist_link_added_desc ON album_artist_link (artist_id, created_at DESC, album_id);

CREATE INDEX IF NOT EXISTS idx_album_artist_link_year ON album_artist_link (artist_id, year IS NULL, year, album_id);

CREATE INDEX IF NOT EXISTS idx_album_artist_link_year_desc ON album_artist_link (artist_id, year IS NULL DESC, year DESC, album_id);

-- Inserting an album id into `album_listing_refresh` rewrites that album's
-- row of `album_listing` and its links from the album, its releases and its
-- credits, writing the row only where a value differs and adding and removing
-- only the links that differ: a release added to an album of many changes
-- none of its other releases' listing rows. The links go with the listing
-- row when the album's last release does, and a new link takes the row's
-- values.
CREATE VIEW IF NOT EXISTS album_listing_refresh AS SELECT NULL AS album_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS album_listing_refresh INSTEAD OF INSERT ON album_listing_refresh
BEGIN
    DELETE FROM album_listing WHERE album_id = NEW.album_id
        AND NOT EXISTS (SELECT 1 FROM releases WHERE album_id = NEW.album_id);
    INSERT INTO album_listing (album_id, title_sort_key, year, created_at, credit_sort_key, credit_artist_ids)
    SELECT album_id, bae_name_key(title, language), year, created_at,
           bae_name_list_key(credit_sort_names, language), credit_artist_ids
    FROM (
        SELECT a.id AS album_id, a.title, a.year, a.created_at,
               (SELECT GROUP_CONCAT(COALESCE(credited.sort_name, credited.name), char(31) ORDER BY credit.position)
                FROM album_artists credit
                JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id
                JOIN artists credited ON credited.id = shown.shown_artist_id
                WHERE credit.album_id = a.id) AS credit_sort_names,
               (SELECT GROUP_CONCAT(credited.id, char(31) ORDER BY credit.position)
                FROM album_artists credit
                JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id
                JOIN artists credited ON credited.id = shown.shown_artist_id
                WHERE credit.album_id = a.id) AS credit_artist_ids
        FROM albums a
        WHERE a.id = NEW.album_id AND EXISTS (SELECT 1 FROM releases WHERE album_id = a.id)
    ) CROSS JOIN (SELECT language FROM name_sort)
    WHERE true
    ON CONFLICT (album_id) DO UPDATE SET
        title_sort_key = excluded.title_sort_key,
        year = excluded.year,
        created_at = excluded.created_at,
        credit_sort_key = excluded.credit_sort_key,
        credit_artist_ids = excluded.credit_artist_ids
    WHERE album_listing.title_sort_key IS NOT excluded.title_sort_key
       OR album_listing.year IS NOT excluded.year
       OR album_listing.created_at IS NOT excluded.created_at
       OR album_listing.credit_sort_key IS NOT excluded.credit_sort_key
       OR album_listing.credit_artist_ids IS NOT excluded.credit_artist_ids;
    DELETE FROM album_artist_link WHERE album_id = NEW.album_id
        AND artist_id NOT IN (
            SELECT shown.shown_artist_id FROM album_artists credit
            JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id
            WHERE credit.album_id = NEW.album_id);
    INSERT OR IGNORE INTO album_artist_link (artist_id, album_id, title_sort_key, year, created_at)
    SELECT DISTINCT shown.shown_artist_id, listing.album_id, listing.title_sort_key, listing.year, listing.created_at
    FROM album_listing listing
    JOIN album_artists credit ON credit.album_id = listing.album_id
    JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id
    WHERE listing.album_id = NEW.album_id;
END;

-- A listing row's title key, year or date added changing moves its links with it.
CREATE TRIGGER IF NOT EXISTS album_listing_values_changed AFTER UPDATE OF title_sort_key, year, created_at ON album_listing
WHEN OLD.title_sort_key IS NOT NEW.title_sort_key
  OR OLD.year IS NOT NEW.year
  OR OLD.created_at IS NOT NEW.created_at
BEGIN
    UPDATE album_artist_link SET title_sort_key = NEW.title_sort_key, year = NEW.year, created_at = NEW.created_at
    WHERE album_id = NEW.album_id;
END;

CREATE TRIGGER IF NOT EXISTS album_listing_added AFTER INSERT ON albums
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS album_listing_changed AFTER UPDATE OF title, year, created_at ON albums
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.id);
END;

-- A release arriving or moving lists its albums through `release_added` and
-- `release_changed` (`storage.sql`), which list the release in the same step.

CREATE TRIGGER IF NOT EXISTS album_release_removed AFTER DELETE ON releases
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (OLD.album_id);
END;

CREATE TRIGGER IF NOT EXISTS album_credit_added AFTER INSERT ON album_artists
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.album_id);
END;

CREATE TRIGGER IF NOT EXISTS album_credit_changed AFTER UPDATE OF artist_id ON album_artists
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.album_id);
END;

CREATE TRIGGER IF NOT EXISTS album_credit_removed AFTER DELETE ON album_artists
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (OLD.album_id);
END;

-- An artist arriving moves every album it is credited on, and every album
-- crediting an artist that shows as it, which waited for its name. An
-- artist's shown artist changing moves every album it is credited on.
CREATE TRIGGER IF NOT EXISTS album_credit_shown_added AFTER INSERT ON artist_shown_as
BEGIN
    INSERT INTO album_listing_refresh (album_id)
    SELECT DISTINCT credit.album_id FROM album_artists credit
    WHERE credit.artist_id = NEW.artist_id
       OR credit.artist_id IN (SELECT artist_id FROM artist_shown_as WHERE shown_artist_id = NEW.artist_id);
END;

CREATE TRIGGER IF NOT EXISTS album_credit_shown_changed AFTER UPDATE OF shown_artist_id ON artist_shown_as
BEGIN
    INSERT INTO album_listing_refresh (album_id)
    SELECT DISTINCT album_id FROM album_artists WHERE artist_id = NEW.artist_id;
END;

-- A renamed artist moves the albums whose credits show it, as
-- `artist_listing_renamed` (`artist.sql`) moves it in the artist list.
CREATE TRIGGER IF NOT EXISTS album_listing_artist_renamed AFTER UPDATE OF name, sort_name ON artists
BEGIN
    INSERT INTO album_listing_refresh (album_id)
    SELECT DISTINCT credit.album_id FROM artist_shown_as shown
    JOIN album_artists credit ON credit.artist_id = shown.artist_id
    WHERE shown.shown_artist_id = NEW.id;
END;
