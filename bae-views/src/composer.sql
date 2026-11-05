-- The composer list (§2.4) counts three things for each artist it shows: the
-- works it is credited on, the releases a track of which performs one of them,
-- and its catalog role credits that none of its works link. Each is a view,
-- kept in a table of its own by the triggers below; the list's counts follow
-- those tables.

-- Each work and each artist shown credited on it.
CREATE VIEW IF NOT EXISTS work_composers AS
SELECT DISTINCT shown.shown_artist_id AS artist_id, wa.work_id
FROM work_artists wa
JOIN artist_shown_as shown ON shown.artist_id = wa.artist_id;

-- Each release and each artist shown credited on a work one of its tracks
-- performs.
CREATE VIEW IF NOT EXISTS release_work_composers AS
SELECT DISTINCT shown.shown_artist_id AS artist_id, t.release_id
FROM tracks t
JOIN track_works tw ON tw.track_id = t.id
JOIN work_artists wa ON wa.work_id = tw.work_id
JOIN artist_shown_as shown ON shown.artist_id = wa.artist_id;

-- Each catalog role credit no work of its artist links, with the artist it
-- shows as and the release it is on: a release's credit when no track of the
-- release performs a work that artist is credited on, a track's when the track
-- performs none.
CREATE VIEW IF NOT EXISTS unlinked_composer_credits AS
SELECT rar.id AS credit_id, credited.shown_artist_id AS artist_id, rar.release_id
FROM release_artist_roles rar
JOIN artist_shown_as credited ON credited.artist_id = rar.artist_id
WHERE NOT EXISTS (
    SELECT 1 FROM tracks t
    JOIN track_works tw ON tw.track_id = t.id
    JOIN work_artists wa ON wa.work_id = tw.work_id
    WHERE t.release_id = rar.release_id
      AND (SELECT shown_artist_id FROM artist_shown_as WHERE artist_id = wa.artist_id)
          = credited.shown_artist_id
)
UNION ALL
SELECT tar.id, credited.shown_artist_id, t.release_id
FROM track_artist_roles tar
JOIN artist_shown_as credited ON credited.artist_id = tar.artist_id
JOIN tracks t ON t.id = tar.track_id
WHERE NOT EXISTS (
    SELECT 1 FROM track_works tw
    JOIN work_artists wa ON wa.work_id = tw.work_id
    WHERE tw.track_id = tar.track_id
      AND (SELECT shown_artist_id FROM artist_shown_as WHERE artist_id = wa.artist_id)
          = credited.shown_artist_id
);

-- The rows of `work_composers`.
CREATE TABLE IF NOT EXISTS composer_work (
    artist_id TEXT NOT NULL,
    work_id TEXT NOT NULL,
    PRIMARY KEY (artist_id, work_id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_composer_work_work ON composer_work (work_id);

-- The rows of `release_work_composers`.
CREATE TABLE IF NOT EXISTS composer_release (
    artist_id TEXT NOT NULL,
    release_id TEXT NOT NULL,
    PRIMARY KEY (artist_id, release_id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_composer_release_release ON composer_release (release_id);

-- The rows of `unlinked_composer_credits`.
CREATE TABLE IF NOT EXISTS composer_unlinked_credit (
    credit_id TEXT PRIMARY KEY,
    artist_id TEXT NOT NULL,
    release_id TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_composer_unlinked_credit_artist ON composer_unlinked_credit (artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_unlinked_credit_release ON composer_unlinked_credit (release_id);

-- Each artist the composer list shows, with its name for the list's order and
-- the three counts: a row while any count is above zero. An index serves each
-- of the list's sorts, ties broken by name, then id.
CREATE TABLE IF NOT EXISTS composer_listing (
    artist_id TEXT PRIMARY KEY REFERENCES artists (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    work_count INTEGER NOT NULL CHECK (work_count >= 0),
    linked_release_count INTEGER NOT NULL CHECK (linked_release_count >= 0),
    unlinked_credit_count INTEGER NOT NULL CHECK (unlinked_credit_count >= 0)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_composer_listing_name ON composer_listing (name, artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_listing_name_desc ON composer_listing (name DESC, artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_listing_work_count ON composer_listing (work_count, name, artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_listing_work_count_desc ON composer_listing (work_count DESC, name, artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_listing_release_count ON composer_listing (linked_release_count, name, artist_id);

CREATE INDEX IF NOT EXISTS idx_composer_listing_release_count_desc ON composer_listing (linked_release_count DESC, name, artist_id);

-- Each row added to or removed from the three tables counts in or out of its
-- artist's listing row. An artist whose own row has not arrived yet gets its
-- listing row when it does (`composer_artist_shown_added`).
CREATE TRIGGER IF NOT EXISTS composer_work_added AFTER INSERT ON composer_work
BEGIN
    INSERT INTO composer_listing (artist_id, name, work_count, linked_release_count, unlinked_credit_count)
    SELECT id, name, 1, 0, 0 FROM artists WHERE id = NEW.artist_id
    ON CONFLICT (artist_id) DO UPDATE SET work_count = work_count + 1;
END;

CREATE TRIGGER IF NOT EXISTS composer_work_removed AFTER DELETE ON composer_work
BEGIN
    UPDATE composer_listing SET work_count = work_count - 1 WHERE artist_id = OLD.artist_id;
    DELETE FROM composer_listing WHERE artist_id = OLD.artist_id
        AND work_count = 0 AND linked_release_count = 0 AND unlinked_credit_count = 0;
END;

CREATE TRIGGER IF NOT EXISTS composer_release_added AFTER INSERT ON composer_release
BEGIN
    INSERT INTO composer_listing (artist_id, name, work_count, linked_release_count, unlinked_credit_count)
    SELECT id, name, 0, 1, 0 FROM artists WHERE id = NEW.artist_id
    ON CONFLICT (artist_id) DO UPDATE SET linked_release_count = linked_release_count + 1;
END;

CREATE TRIGGER IF NOT EXISTS composer_release_removed AFTER DELETE ON composer_release
BEGIN
    UPDATE composer_listing SET linked_release_count = linked_release_count - 1 WHERE artist_id = OLD.artist_id;
    DELETE FROM composer_listing WHERE artist_id = OLD.artist_id
        AND work_count = 0 AND linked_release_count = 0 AND unlinked_credit_count = 0;
END;

CREATE TRIGGER IF NOT EXISTS composer_unlinked_credit_added AFTER INSERT ON composer_unlinked_credit
BEGIN
    INSERT INTO composer_listing (artist_id, name, work_count, linked_release_count, unlinked_credit_count)
    SELECT id, name, 0, 0, 1 FROM artists WHERE id = NEW.artist_id
    ON CONFLICT (artist_id) DO UPDATE SET unlinked_credit_count = unlinked_credit_count + 1;
END;

CREATE TRIGGER IF NOT EXISTS composer_unlinked_credit_removed AFTER DELETE ON composer_unlinked_credit
BEGIN
    UPDATE composer_listing SET unlinked_credit_count = unlinked_credit_count - 1 WHERE artist_id = OLD.artist_id;
    DELETE FROM composer_listing WHERE artist_id = OLD.artist_id
        AND work_count = 0 AND linked_release_count = 0 AND unlinked_credit_count = 0;
END;

-- Inserting a release id into `composer_release_refresh` rewrites that
-- release's rows of `composer_release` and `composer_unlinked_credit` from
-- their views, adding and removing only the rows that differ; inserting a work
-- id into `composer_work_refresh` does the same for that work's rows of
-- `composer_work`. These two views hold no rows: they name the statements
-- their INSTEAD OF triggers run, which the triggers on the library tables
-- share.
CREATE VIEW IF NOT EXISTS composer_release_refresh AS SELECT NULL AS release_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS composer_release_refresh INSTEAD OF INSERT ON composer_release_refresh
BEGIN
    DELETE FROM composer_release WHERE release_id = NEW.release_id
        AND artist_id NOT IN (SELECT artist_id FROM release_work_composers WHERE release_id = NEW.release_id);
    INSERT OR IGNORE INTO composer_release (artist_id, release_id)
    SELECT artist_id, release_id FROM release_work_composers WHERE release_id = NEW.release_id;
    DELETE FROM composer_unlinked_credit WHERE release_id = NEW.release_id
        AND (credit_id, artist_id) NOT IN (
            SELECT credit_id, artist_id FROM unlinked_composer_credits WHERE release_id = NEW.release_id);
    INSERT OR IGNORE INTO composer_unlinked_credit (credit_id, artist_id, release_id)
    SELECT credit_id, artist_id, release_id FROM unlinked_composer_credits WHERE release_id = NEW.release_id;
END;

CREATE VIEW IF NOT EXISTS composer_work_refresh AS SELECT NULL AS work_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS composer_work_refresh INSTEAD OF INSERT ON composer_work_refresh
BEGIN
    DELETE FROM composer_work WHERE work_id = NEW.work_id
        AND artist_id NOT IN (SELECT artist_id FROM work_composers WHERE work_id = NEW.work_id);
    INSERT OR IGNORE INTO composer_work (artist_id, work_id)
    SELECT artist_id, work_id FROM work_composers WHERE work_id = NEW.work_id;
END;

-- Each work and each release a track of which performs it.
CREATE VIEW IF NOT EXISTS work_releases AS
SELECT DISTINCT tw.work_id, t.release_id FROM track_works tw JOIN tracks t ON t.id = tw.track_id;

-- Each release an artist is credited on: on a work one of its tracks
-- performs, or with a catalog role on the release or one of its tracks.
CREATE VIEW IF NOT EXISTS artist_credited_releases AS
SELECT wa.artist_id, t.release_id
FROM work_artists wa JOIN track_works tw ON tw.work_id = wa.work_id JOIN tracks t ON t.id = tw.track_id
UNION
SELECT artist_id, release_id FROM release_artist_roles
UNION
SELECT tar.artist_id, t.release_id FROM track_artist_roles tar JOIN tracks t ON t.id = tar.track_id;

-- An artist's shown row arriving gives it a listing row for what was
-- already counted under it, then counts its own credits. It fires on the
-- shown row rather than the artist, because the counts read
-- `work_composers`, which joins `artist_shown_as`: the row's insert orders
-- them after it.
CREATE TRIGGER IF NOT EXISTS composer_artist_shown_added AFTER INSERT ON artist_shown_as
BEGIN
    INSERT INTO composer_listing (artist_id, name, work_count, linked_release_count, unlinked_credit_count)
    SELECT artist.id, artist.name,
           (SELECT COUNT(*) FROM composer_work WHERE artist_id = artist.id),
           (SELECT COUNT(*) FROM composer_release WHERE artist_id = artist.id),
           (SELECT COUNT(*) FROM composer_unlinked_credit WHERE artist_id = artist.id)
    FROM artists artist
    WHERE artist.id = NEW.artist_id
      AND (EXISTS (SELECT 1 FROM composer_work WHERE artist_id = artist.id)
        OR EXISTS (SELECT 1 FROM composer_release WHERE artist_id = artist.id)
        OR EXISTS (SELECT 1 FROM composer_unlinked_credit WHERE artist_id = artist.id));
    INSERT INTO composer_work_refresh (work_id)
    SELECT DISTINCT work_id FROM work_artists WHERE artist_id = NEW.artist_id;
    INSERT INTO composer_release_refresh (release_id)
    SELECT DISTINCT release_id FROM artist_credited_releases WHERE artist_id = NEW.artist_id;
END;

-- An artist's shown artist moving rewrites the composer rows of every work
-- and release it is credited on, as `album_credit_shown_changed` rewrites
-- its albums. An artist whose shown artist stayed changes no row.
CREATE TRIGGER IF NOT EXISTS composer_artist_shown_changed AFTER UPDATE OF shown_artist_id ON artist_shown_as
WHEN OLD.shown_artist_id IS NOT NEW.shown_artist_id
BEGIN
    INSERT INTO composer_work_refresh (work_id)
    SELECT DISTINCT work_id FROM work_artists WHERE artist_id = NEW.artist_id;
    INSERT INTO composer_release_refresh (release_id)
    SELECT DISTINCT release_id FROM artist_credited_releases WHERE artist_id = NEW.artist_id;
END;

CREATE TRIGGER IF NOT EXISTS artist_renamed AFTER UPDATE OF name ON artists
BEGIN
    UPDATE composer_listing SET name = NEW.name WHERE artist_id = NEW.id;
END;

-- A work's credited artists change its rows and those of every release
-- performing it.
CREATE TRIGGER IF NOT EXISTS work_artist_added AFTER INSERT ON work_artists
BEGIN
    INSERT INTO composer_work_refresh (work_id) VALUES (NEW.work_id);
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM work_releases WHERE work_id = NEW.work_id;
END;

CREATE TRIGGER IF NOT EXISTS work_artist_changed AFTER UPDATE OF work_id, artist_id ON work_artists
BEGIN
    INSERT INTO composer_work_refresh (work_id) VALUES (OLD.work_id);
    INSERT INTO composer_work_refresh (work_id) VALUES (NEW.work_id);
    INSERT INTO composer_release_refresh (release_id) SELECT DISTINCT release_id FROM work_releases WHERE work_id IN (OLD.work_id, NEW.work_id);
END;

CREATE TRIGGER IF NOT EXISTS work_artist_removed AFTER DELETE ON work_artists
BEGIN
    INSERT INTO composer_work_refresh (work_id) VALUES (OLD.work_id);
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM work_releases WHERE work_id = OLD.work_id;
END;

-- What a track performs changes its release's rows. Deleting a track deletes
-- its links and credits first, which can no longer find its release, so the
-- track's own trigger rewrites the release.
CREATE TRIGGER IF NOT EXISTS track_work_added AFTER INSERT ON track_works
BEGIN
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM tracks WHERE id = NEW.track_id;
END;

CREATE TRIGGER IF NOT EXISTS track_work_changed AFTER UPDATE OF track_id, work_id ON track_works
BEGIN
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM tracks WHERE id = OLD.track_id;
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM tracks WHERE id = NEW.track_id;
END;

CREATE TRIGGER IF NOT EXISTS track_work_removed AFTER DELETE ON track_works
BEGIN
    INSERT INTO composer_release_refresh (release_id) SELECT release_id FROM tracks WHERE id = OLD.track_id;
END;

-- A track arriving changes its release's rows only when a link or a credit of
-- it arrived first.
CREATE TRIGGER IF NOT EXISTS track_added AFTER INSERT ON tracks
WHEN EXISTS (SELECT 1 FROM track_works WHERE track_id = NEW.id)
  OR EXISTS (SELECT 1 FROM track_artist_roles WHERE track_id = NEW.id)
BEGIN
    INSERT INTO composer_release_refresh (release_id) VALUES (NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS track_moved AFTER UPDATE OF release_id ON tracks
BEGIN
    INSERT INTO composer_release_refresh (release_id) VALUES (OLD.release_id);
    INSERT INTO composer_release_refresh (release_id) VALUES (NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS track_removed AFTER DELETE ON tracks
BEGIN
    INSERT INTO composer_release_refresh (release_id) VALUES (OLD.release_id);
END;

-- Whether a role credit is linked depends on the release's works, which a
-- write of the credit itself doesn't change: only its own row follows.
CREATE TRIGGER IF NOT EXISTS release_role_added AFTER INSERT ON release_artist_roles
BEGIN
    INSERT INTO composer_unlinked_credit (credit_id, artist_id, release_id)
    SELECT credit_id, artist_id, release_id FROM unlinked_composer_credits WHERE credit_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS release_role_changed AFTER UPDATE OF id, release_id, artist_id ON release_artist_roles
BEGIN
    DELETE FROM composer_unlinked_credit WHERE credit_id = OLD.id;
    INSERT INTO composer_unlinked_credit (credit_id, artist_id, release_id)
    SELECT credit_id, artist_id, release_id FROM unlinked_composer_credits WHERE credit_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS release_role_removed AFTER DELETE ON release_artist_roles
BEGIN
    DELETE FROM composer_unlinked_credit WHERE credit_id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS track_role_added AFTER INSERT ON track_artist_roles
BEGIN
    INSERT INTO composer_unlinked_credit (credit_id, artist_id, release_id)
    SELECT credit_id, artist_id, release_id FROM unlinked_composer_credits WHERE credit_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS track_role_changed AFTER UPDATE OF id, track_id, artist_id ON track_artist_roles
BEGIN
    DELETE FROM composer_unlinked_credit WHERE credit_id = OLD.id;
    INSERT INTO composer_unlinked_credit (credit_id, artist_id, release_id)
    SELECT credit_id, artist_id, release_id FROM unlinked_composer_credits WHERE credit_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS track_role_removed AFTER DELETE ON track_artist_roles
BEGIN
    DELETE FROM composer_unlinked_credit WHERE credit_id = OLD.id;
END;
