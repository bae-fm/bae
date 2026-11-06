-- A composer's page (§2.4) lists its works, then its unlinked release
-- credits, then its unlinked track credits, in one order that reads works'
-- titles and parts and the credits' albums, releases and tracks. No index
-- reaches across those, so `composer_detail_row` holds each row of each page
-- with what it sorts by, kept by the triggers below.

-- Each row of each composer's page, in the page's order. In the works, each
-- credited work is listed under every work the library holds that it is part
-- of, each of those heading its group whether the composer is credited on it
-- or not, and a credited work part of none heads a group of its own; a group
-- sorts by its heading work's title, its works heading first, then by title.
-- Each unlinked credit is listed under its release, the releases by album
-- title, then by album, so two albums sharing a title don't interleave, then
-- by the release's place among its album's; the credits by track and then by
-- their own order. Titles compare without case, as the album grid's do.
CREATE TABLE IF NOT EXISTS composer_detail_row (
    -- The row's number: what a screen tells it apart by.
    entry       INTEGER PRIMARY KEY,
    -- The composer, as the composer list shows it.
    artist_id   TEXT NOT NULL,
    -- The part of the page: 0 the works, 1 the release credits, 2 the track
    -- credits.
    section     INTEGER NOT NULL CHECK (section IN (0, 1, 2)),
    -- The row's group: in the works, the work heading it; for a credit, its
    -- release.
    group_id    TEXT NOT NULL,
    -- The heading work's title, or the release's album's.
    group_title TEXT NOT NULL COLLATE NOCASE,
    -- For a credit, the release's album.
    group_album TEXT,
    -- For a credit, the release's place among its album's (`album_order`).
    group_order REAL,
    -- In the works, 0 for the work heading its group and 1 for a part; for a
    -- track credit, its track's position. NULL for a release credit.
    place       INTEGER,
    -- A work's title.
    title       TEXT COLLATE NOCASE,
    -- A credit's position among its release's or its track's.
    position    INTEGER,
    -- The work or the credit the row shows.
    item_id     TEXT NOT NULL,
    UNIQUE (artist_id, section, group_id, item_id),
    CHECK ((section = 0) = (title IS NOT NULL)),
    CHECK ((section = 0) = (group_album IS NULL)),
    CHECK ((section = 0) = (group_order IS NULL)),
    CHECK ((section = 0) = (position IS NULL)),
    CHECK ((section = 1) = (place IS NULL)),
    CHECK (section <> 0 OR place = (group_id <> item_id))
) STRICT;

CREATE INDEX IF NOT EXISTS idx_composer_detail_row_order ON composer_detail_row
    (artist_id, section, group_title, group_album, group_order, group_id, place, title, position, item_id);

CREATE INDEX IF NOT EXISTS idx_composer_detail_row_item ON composer_detail_row (item_id);

-- The works rows of every composer's page: each credited work under each
-- work it is part of, each such work heading its group, and each credited
-- work part of none the library holds heading its own. A refresh reads it
-- for one artist and one work, which SQLite pushes into each arm; the
-- heading arm is read from the heading work through its parts (CROSS JOIN
-- keeps that order), so it costs the work's parts rather than every work
-- the artist is credited on.
CREATE VIEW IF NOT EXISTS composer_detail_works AS
SELECT credited.artist_id, part.parent_work_id AS group_id, parent.title AS group_title,
       1 AS place, work.title, credited.work_id AS item_id
FROM composer_work credited
JOIN work_parts part ON part.child_work_id = credited.work_id
JOIN works parent ON parent.id = part.parent_work_id
JOIN works work ON work.id = credited.work_id
UNION
SELECT credited.artist_id, parent.id, parent.title, 0, parent.title, parent.id
FROM works parent
CROSS JOIN work_parts part ON part.parent_work_id = parent.id
CROSS JOIN composer_work credited ON credited.work_id = part.child_work_id
UNION
SELECT credited.artist_id, work.id, work.title, 0, work.title, work.id
FROM composer_work credited
JOIN works work ON work.id = credited.work_id
WHERE NOT EXISTS (
    SELECT 1 FROM work_parts part JOIN works parent ON parent.id = part.parent_work_id
    WHERE part.child_work_id = credited.work_id);

-- The credit rows of every composer's page: each unlinked credit under its
-- release, once its release and album are held.
CREATE VIEW IF NOT EXISTS composer_detail_credits AS
SELECT unlinked.artist_id, 1 AS section, unlinked.release_id AS group_id,
       album.title AS group_title, album.id AS group_album, r.album_order AS group_order,
       NULL AS place, rar.position, unlinked.credit_id AS item_id
FROM composer_unlinked_credit unlinked
JOIN release_artist_roles rar ON rar.id = unlinked.credit_id
JOIN releases r ON r.id = unlinked.release_id
JOIN albums album ON album.id = r.album_id
UNION ALL
SELECT unlinked.artist_id, 2, unlinked.release_id, album.title, album.id, r.album_order,
       t.position, tar.position, unlinked.credit_id
FROM composer_unlinked_credit unlinked
JOIN track_artist_roles tar ON tar.id = unlinked.credit_id
JOIN tracks t ON t.id = tar.track_id
JOIN releases r ON r.id = unlinked.release_id
JOIN albums album ON album.id = r.album_id;

-- Inserting an artist and a work into `composer_detail_work_refresh`
-- rewrites the rows that list that work on that artist's page from
-- `composer_detail_works`, and inserting a credit into
-- `composer_detail_credit_refresh` rewrites the row that lists it from
-- `composer_detail_credits`: each adds and removes only the rows that differ
-- and updates a row in place where only what it sorts by changed. The views
-- hold no rows; they name the statements their INSTEAD OF triggers run.
CREATE VIEW IF NOT EXISTS composer_detail_work_refresh AS SELECT NULL AS artist_id, NULL AS work_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS composer_detail_work_refresh INSTEAD OF INSERT ON composer_detail_work_refresh
BEGIN
    DELETE FROM composer_detail_row
    WHERE artist_id = NEW.artist_id AND section = 0 AND item_id = NEW.work_id
      AND group_id NOT IN (
          SELECT group_id FROM composer_detail_works
          WHERE artist_id = NEW.artist_id AND item_id = NEW.work_id);
    INSERT INTO composer_detail_row (artist_id, section, group_id, group_title, place, title, item_id)
    SELECT artist_id, 0, group_id, group_title, place, title, item_id FROM composer_detail_works
    WHERE artist_id = NEW.artist_id AND item_id = NEW.work_id
    ON CONFLICT (artist_id, section, group_id, item_id) DO UPDATE SET
        group_title = excluded.group_title,
        title = excluded.title
    WHERE composer_detail_row.group_title IS NOT excluded.group_title
       OR composer_detail_row.title IS NOT excluded.title;
END;

CREATE VIEW IF NOT EXISTS composer_detail_credit_refresh AS SELECT NULL AS credit_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS composer_detail_credit_refresh INSTEAD OF INSERT ON composer_detail_credit_refresh
BEGIN
    DELETE FROM composer_detail_row
    WHERE item_id = NEW.credit_id AND section <> 0
      AND (artist_id, section, group_id) NOT IN (
          SELECT artist_id, section, group_id FROM composer_detail_credits
          WHERE item_id = NEW.credit_id);
    INSERT INTO composer_detail_row
        (artist_id, section, group_id, group_title, group_album, group_order, place, position,
         item_id)
    SELECT artist_id, section, group_id, group_title, group_album, group_order, place, position,
           item_id
    FROM composer_detail_credits WHERE item_id = NEW.credit_id
    ON CONFLICT (artist_id, section, group_id, item_id) DO UPDATE SET
        group_title = excluded.group_title,
        group_album = excluded.group_album,
        group_order = excluded.group_order
    WHERE composer_detail_row.group_title IS NOT excluded.group_title
       OR composer_detail_row.group_album IS NOT excluded.group_album
       OR composer_detail_row.group_order IS NOT excluded.group_order;
END;

-- A work credited on an artist or no longer lists that work on the artist's
-- page, and the works it is part of, which head a group while any of their
-- parts is credited there.
CREATE TRIGGER IF NOT EXISTS composer_detail_work_credited AFTER INSERT ON composer_work
BEGIN
    INSERT INTO composer_detail_work_refresh (artist_id, work_id)
    SELECT NEW.artist_id, NEW.work_id
    UNION ALL
    SELECT NEW.artist_id, parent_work_id FROM work_parts WHERE child_work_id = NEW.work_id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_work_uncredited AFTER DELETE ON composer_work
BEGIN
    INSERT INTO composer_detail_work_refresh (artist_id, work_id)
    SELECT OLD.artist_id, OLD.work_id
    UNION ALL
    SELECT OLD.artist_id, parent_work_id FROM work_parts WHERE child_work_id = OLD.work_id;
END;

-- A part joining a work moves the part on every page it is credited on, and
-- the work's heading there. No write takes a part out of a work or changes
-- which work it is part of: its row's id is made of the two works.
CREATE TRIGGER IF NOT EXISTS composer_detail_part_added AFTER INSERT ON work_parts
BEGIN
    INSERT INTO composer_detail_work_refresh (artist_id, work_id)
    SELECT artist_id, NEW.child_work_id FROM composer_work WHERE work_id = NEW.child_work_id
    UNION ALL
    SELECT artist_id, NEW.parent_work_id FROM composer_work WHERE work_id = NEW.child_work_id;
END;

-- A work arriving or retitled changes every row that shows its title:
-- its own on the pages it is credited on or heads a group on, and its parts'
-- on theirs, which it heads, or which headed their own while it was missing.
CREATE VIEW IF NOT EXISTS composer_detail_title_refresh AS SELECT NULL AS work_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS composer_detail_title_refresh INSTEAD OF INSERT ON composer_detail_title_refresh
BEGIN
    INSERT INTO composer_detail_work_refresh (artist_id, work_id)
    SELECT artist_id, NEW.work_id FROM composer_work WHERE work_id = NEW.work_id
    UNION
    SELECT credited.artist_id, NEW.work_id FROM work_parts part
    JOIN composer_work credited ON credited.work_id = part.child_work_id
    WHERE part.parent_work_id = NEW.work_id
    UNION
    SELECT credited.artist_id, credited.work_id FROM work_parts part
    JOIN composer_work credited ON credited.work_id = part.child_work_id
    WHERE part.parent_work_id = NEW.work_id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_work_added AFTER INSERT ON works
BEGIN
    INSERT INTO composer_detail_title_refresh (work_id) VALUES (NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_work_retitled AFTER UPDATE OF title ON works
WHEN OLD.title IS NOT NEW.title
BEGIN
    INSERT INTO composer_detail_title_refresh (work_id) VALUES (NEW.id);
END;

-- An unlinked credit counted in or out lists or unlists it.
CREATE TRIGGER IF NOT EXISTS composer_detail_credit_added AFTER INSERT ON composer_unlinked_credit
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id) VALUES (NEW.credit_id);
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_credit_removed AFTER DELETE ON composer_unlinked_credit
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id) VALUES (OLD.credit_id);
END;

-- What a credit sorts by changing moves it: its album arriving or retitled,
-- or its release arriving, moving to another album or to another place
-- among its album's. A track's place and a credit's own position are written
-- once, with the row.
CREATE TRIGGER IF NOT EXISTS composer_detail_album_added AFTER INSERT ON albums
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id)
    SELECT unlinked.credit_id FROM releases r
    JOIN composer_unlinked_credit unlinked ON unlinked.release_id = r.id
    WHERE r.album_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_album_retitled AFTER UPDATE OF title ON albums
WHEN OLD.title IS NOT NEW.title
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id)
    SELECT unlinked.credit_id FROM releases r
    JOIN composer_unlinked_credit unlinked ON unlinked.release_id = r.id
    WHERE r.album_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_release_added AFTER INSERT ON releases
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id)
    SELECT credit_id FROM composer_unlinked_credit WHERE release_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_release_moved AFTER UPDATE OF album_id, album_order ON releases
WHEN OLD.album_id IS NOT NEW.album_id OR OLD.album_order IS NOT NEW.album_order
BEGIN
    INSERT INTO composer_detail_credit_refresh (credit_id)
    SELECT credit_id FROM composer_unlinked_credit WHERE release_id = NEW.id;
END;

