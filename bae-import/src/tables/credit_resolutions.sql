-- Which library artists the records' rule (`bae_records::credit_artists_on`)
-- names for each artist credit a candidate's draft holds, stored once per
-- distinct credit as the rule reads one: its folded name and its two catalog
-- ids. The rule reads nothing else of a credit, so two credits alike in these
-- name the same artists.
--
-- Every write that changes a credit or an artist the rule can name for it
-- adds the credit to `import_credit_resolution_due` through the triggers
-- below, whether this device made the write or coven applies another
-- device's. bae-import's before-commit function answers each due credit in
-- the same transaction and empties the list, so no transaction commits with
-- a due credit and no reader sees a credit without its answer.

-- One row per distinct credit that a draft holds, answered.
-- Its artists are the answer: none for a new artist, one for the library
-- artist the credit names, several for the ones it leaves ambiguous.
CREATE TABLE IF NOT EXISTS import_credit_resolution (
    id                    INTEGER PRIMARY KEY,
    name_key              TEXT NOT NULL,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT
) STRICT;

-- One answer per credit: SQLite takes two NULLs in a plain unique index as
-- different, and the JSON array tells NULL from every text.
CREATE UNIQUE INDEX IF NOT EXISTS import_credit_resolution_by_credit
    ON import_credit_resolution (json_array(name_key, musicbrainz_artist_id, discogs_artist_id));

-- The indexes the triggers below find the answers an artist's change reaches
-- by: its names, its catalog ids, its own id as one a catalog id derives,
-- and Various Artists.
CREATE INDEX IF NOT EXISTS import_credit_resolution_by_name_key
    ON import_credit_resolution (name_key);

CREATE INDEX IF NOT EXISTS import_credit_resolution_by_musicbrainz
    ON import_credit_resolution (musicbrainz_artist_id);

CREATE INDEX IF NOT EXISTS import_credit_resolution_by_discogs
    ON import_credit_resolution (discogs_artist_id);

CREATE INDEX IF NOT EXISTS import_credit_resolution_by_musicbrainz_identity
    ON import_credit_resolution (bae_artist_identity(musicbrainz_artist_id, NULL));

CREATE INDEX IF NOT EXISTS import_credit_resolution_by_discogs_identity
    ON import_credit_resolution (bae_artist_identity(NULL, discogs_artist_id));

CREATE INDEX IF NOT EXISTS import_credit_resolution_by_various_artists
    ON import_credit_resolution (bae_artist_is_various_artists(musicbrainz_artist_id, discogs_artist_id));

-- Each library artist an answer names, in the rule's order, with the fields
-- the rule gave it: an artist that one catalog id names on several artists
-- holds the ids and sort name it takes from the others.
CREATE TABLE IF NOT EXISTS import_credit_resolution_artist (
    resolution            INTEGER NOT NULL
        REFERENCES import_credit_resolution (id) ON DELETE CASCADE,
    position              INTEGER NOT NULL CHECK (position >= 0),
    artist_id             TEXT NOT NULL,
    name                  TEXT NOT NULL,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (resolution, position)
) STRICT;

CREATE INDEX IF NOT EXISTS import_credit_resolution_artist_by_artist
    ON import_credit_resolution_artist (artist_id);

-- The credits the transaction's writes may have changed the answer of, a
-- credit once for each write that reached it. The before-commit function
-- empties it before every commit.
CREATE TABLE IF NOT EXISTS import_credit_resolution_due (
    name_key              TEXT NOT NULL,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT
) STRICT;

-- A credit a draft gains, loses or changes is due: the old one
-- and the new one.

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_draft_album_credit_added
AFTER INSERT ON import_candidate_album_artist_assignment
WHEN NEW.assignment_kind = 'credit'
BEGIN
    INSERT INTO import_credit_resolution_due
    VALUES (bae_artist_name_key(NEW.name), NEW.musicbrainz_artist_id, NEW.discogs_artist_id);
END;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_draft_album_credit_removed
AFTER DELETE ON import_candidate_album_artist_assignment
WHEN OLD.assignment_kind = 'credit'
BEGIN
    INSERT INTO import_credit_resolution_due
    VALUES (bae_artist_name_key(OLD.name), OLD.musicbrainz_artist_id, OLD.discogs_artist_id);
END;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_draft_album_credit_changed
AFTER UPDATE OF assignment_kind, name, musicbrainz_artist_id, discogs_artist_id
ON import_candidate_album_artist_assignment
BEGIN
    INSERT INTO import_credit_resolution_due
    SELECT bae_artist_name_key(OLD.name), OLD.musicbrainz_artist_id, OLD.discogs_artist_id
    WHERE OLD.assignment_kind = 'credit';
    INSERT INTO import_credit_resolution_due
    SELECT bae_artist_name_key(NEW.name), NEW.musicbrainz_artist_id, NEW.discogs_artist_id
    WHERE NEW.assignment_kind = 'credit';
END;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_draft_track_credit_added
AFTER INSERT ON import_candidate_track_artist_assignment
WHEN NEW.assignment_kind = 'credit'
BEGIN
    INSERT INTO import_credit_resolution_due
    VALUES (bae_artist_name_key(NEW.name), NEW.musicbrainz_artist_id, NEW.discogs_artist_id);
END;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_draft_track_credit_removed
AFTER DELETE ON import_candidate_track_artist_assignment
WHEN OLD.assignment_kind = 'credit'
BEGIN
    INSERT INTO import_credit_resolution_due
    VALUES (bae_artist_name_key(OLD.name), OLD.musicbrainz_artist_id, OLD.discogs_artist_id);
END;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_draft_track_credit_changed
AFTER UPDATE OF assignment_kind, name, musicbrainz_artist_id, discogs_artist_id
ON import_candidate_track_artist_assignment
BEGIN
    INSERT INTO import_credit_resolution_due
    SELECT bae_artist_name_key(OLD.name), OLD.musicbrainz_artist_id, OLD.discogs_artist_id
    WHERE OLD.assignment_kind = 'credit';
    INSERT INTO import_credit_resolution_due
    SELECT bae_artist_name_key(NEW.name), NEW.musicbrainz_artist_id, NEW.discogs_artist_id
    WHERE NEW.assignment_kind = 'credit';
END;

-- Inserting an artist's id, with the name key and catalog ids it held before
-- a change, marks due every stored answer the change can reach. The view
-- holds no rows: it names the statement its INSTEAD OF trigger runs, which
-- the triggers on `artists` and `artist_shown_as` share.
--
-- The rule names an artist for a credit by what the artist and the artists
-- it shows as hold: its folded name and the printed names of `artist_credits`,
-- its catalog ids, its own id as the one a catalog id derives, and the other
-- catalog's Various Artists id for a credit naming one alone. So the
-- statement marks every answer whose credit carries a name key, a catalog id
-- or a derived id of the artist or of an artist that shows as it, old or new;
-- every answer of a Various Artists credit when one of them holds a Various
-- Artists id; and every answer that names the artist. These are more answers
-- than the change alters: the statement reaches them and the rule decides.
CREATE VIEW IF NOT EXISTS import_credit_resolution_follow AS
SELECT NULL AS artist_id, NULL AS name_key, NULL AS musicbrainz_artist_id,
       NULL AS discogs_artist_id
WHERE 0;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_follow
INSTEAD OF INSERT ON import_credit_resolution_follow
BEGIN
    INSERT INTO import_credit_resolution_due
    SELECT r.name_key, r.musicbrainz_artist_id, r.discogs_artist_id
    FROM import_credit_resolution AS r
    WHERE r.name_key IN (
              SELECT NEW.name_key
              UNION SELECT a.name_key FROM artists AS a
                    WHERE a.id = NEW.artist_id
                       OR a.id IN (SELECT artist_id FROM artist_shown_as
                                   WHERE shown_artist_id = NEW.artist_id)
              UNION SELECT printed.name_key FROM artist_credits AS printed
                    WHERE printed.artist_id = NEW.artist_id
                       OR printed.artist_id IN (SELECT artist_id FROM artist_shown_as
                                                WHERE shown_artist_id = NEW.artist_id))
       OR r.musicbrainz_artist_id IN (
              SELECT NEW.musicbrainz_artist_id
              UNION SELECT a.musicbrainz_artist_id FROM artists AS a
                    WHERE a.id = NEW.artist_id
                       OR a.id IN (SELECT artist_id FROM artist_shown_as
                                   WHERE shown_artist_id = NEW.artist_id))
       OR r.discogs_artist_id IN (
              SELECT NEW.discogs_artist_id
              UNION SELECT a.discogs_artist_id FROM artists AS a
                    WHERE a.id = NEW.artist_id
                       OR a.id IN (SELECT artist_id FROM artist_shown_as
                                   WHERE shown_artist_id = NEW.artist_id))
       OR bae_artist_identity(r.musicbrainz_artist_id, NULL) IN (
              SELECT NEW.artist_id
              UNION SELECT artist_id FROM artist_shown_as WHERE shown_artist_id = NEW.artist_id)
       OR bae_artist_identity(NULL, r.discogs_artist_id) IN (
              SELECT NEW.artist_id
              UNION SELECT artist_id FROM artist_shown_as WHERE shown_artist_id = NEW.artist_id)
       OR (bae_artist_is_various_artists(r.musicbrainz_artist_id, r.discogs_artist_id) = 1
           AND (bae_artist_is_various_artists(NEW.musicbrainz_artist_id, NEW.discogs_artist_id) = 1
                OR EXISTS (SELECT 1 FROM artists AS a
                           WHERE (a.id = NEW.artist_id
                                  OR a.id IN (SELECT artist_id FROM artist_shown_as
                                              WHERE shown_artist_id = NEW.artist_id))
                             AND bae_artist_is_various_artists(a.musicbrainz_artist_id,
                                                               a.discogs_artist_id) = 1)))
       OR r.id IN (SELECT resolution FROM import_credit_resolution_artist
                   WHERE artist_id = NEW.artist_id);
END;

-- An artist arriving, leaving, or changing a column the rule reads or an
-- answer holds.
CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_artist_added AFTER INSERT ON artists
BEGIN
    INSERT INTO import_credit_resolution_follow VALUES (NEW.id, NULL, NULL, NULL);
END;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_artist_removed AFTER DELETE ON artists
BEGIN
    INSERT INTO import_credit_resolution_follow
    VALUES (OLD.id, OLD.name_key, OLD.musicbrainz_artist_id, OLD.discogs_artist_id);
END;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_artist_changed
AFTER UPDATE OF name, name_key, sort_name, musicbrainz_artist_id, discogs_artist_id ON artists
BEGIN
    INSERT INTO import_credit_resolution_follow
    VALUES (OLD.id, OLD.name_key, OLD.musicbrainz_artist_id, OLD.discogs_artist_id);
END;

-- An artist's other printed name arriving, leaving or changing: the name
-- step matches a credit's folded name with it.
CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_printed_name_added
AFTER INSERT ON artist_credits
BEGIN
    INSERT INTO import_credit_resolution_due
    SELECT name_key, musicbrainz_artist_id, discogs_artist_id FROM import_credit_resolution
    WHERE name_key = NEW.name_key;
END;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_printed_name_removed
AFTER DELETE ON artist_credits
BEGIN
    INSERT INTO import_credit_resolution_due
    SELECT name_key, musicbrainz_artist_id, discogs_artist_id FROM import_credit_resolution
    WHERE name_key = OLD.name_key;
END;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_printed_name_changed
AFTER UPDATE OF artist_id, name_key ON artist_credits
BEGIN
    INSERT INTO import_credit_resolution_due
    SELECT name_key, musicbrainz_artist_id, discogs_artist_id FROM import_credit_resolution
    WHERE name_key IN (OLD.name_key, NEW.name_key);
END;

-- An artist showing as another artist from now on, or no longer: what it
-- holds reaches the answers through the artist it shows as, before and
-- after.
CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_shown_as_added
AFTER INSERT ON artist_shown_as
BEGIN
    INSERT INTO import_credit_resolution_follow VALUES (NEW.artist_id, NULL, NULL, NULL);
    INSERT INTO import_credit_resolution_follow VALUES (NEW.shown_artist_id, NULL, NULL, NULL);
END;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_shown_as_removed
AFTER DELETE ON artist_shown_as
BEGIN
    INSERT INTO import_credit_resolution_follow VALUES (OLD.artist_id, NULL, NULL, NULL);
    INSERT INTO import_credit_resolution_follow VALUES (OLD.shown_artist_id, NULL, NULL, NULL);
END;

CREATE TRIGGER IF NOT EXISTS import_credit_resolution_on_shown_as_changed
AFTER UPDATE OF shown_artist_id ON artist_shown_as
WHEN OLD.shown_artist_id IS NOT NEW.shown_artist_id
BEGIN
    INSERT INTO import_credit_resolution_follow VALUES (NEW.artist_id, NULL, NULL, NULL);
    INSERT INTO import_credit_resolution_follow VALUES (OLD.shown_artist_id, NULL, NULL, NULL);
    INSERT INTO import_credit_resolution_follow VALUES (NEW.shown_artist_id, NULL, NULL, NULL);
END;
