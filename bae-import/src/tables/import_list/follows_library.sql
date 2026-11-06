-- The placement follows what this device imported and the library records
-- its rows show: each change to an import's origin, to an artist or credit
-- name a draft prints, or to an imported release's album places again the
-- releases it can move (`import_list_replace`). A change to a column the
-- rules don't read places nothing.

CREATE TRIGGER IF NOT EXISTS import_list_on_origin_added AFTER INSERT ON local_release_origin
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_origin_removed AFTER DELETE ON local_release_origin
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_origin_changed
AFTER UPDATE OF content_hash, release_id ON local_release_origin
WHEN (OLD.content_hash, OLD.release_id) IS NOT (NEW.content_hash, NEW.release_id)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

-- A picked artist's name, or the credit name the draft prints for it; and
-- the name of an artist an imported release's album credits show as, or a
-- credit name the album prints.
CREATE TRIGGER IF NOT EXISTS import_list_on_artist_renamed AFTER UPDATE OF name ON artists
WHEN OLD.name IS NOT NEW.name
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM import_candidate_album_artist_assignment AS a
    JOIN scan_candidate AS c ON c.content_hash = a.content_hash
    WHERE a.artist_id = NEW.id;
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM releases AS r
    JOIN local_release_origin AS o ON o.release_id = r.id
    JOIN scan_candidate AS c ON c.content_hash = o.content_hash
    WHERE r.album_id IN (
          SELECT credit.album_id FROM album_artists AS credit
          JOIN artist_shown_as AS shown_as ON shown_as.artist_id = credit.artist_id
          WHERE shown_as.shown_artist_id = NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_artist_credit_added AFTER INSERT ON artist_credits
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM import_candidate_album_artist_assignment AS a
    JOIN scan_candidate AS c ON c.content_hash = a.content_hash
    WHERE a.artist_id = NEW.artist_id AND a.credit_key = NEW.name_key;
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM releases AS r
    JOIN local_release_origin AS o ON o.release_id = r.id
    JOIN scan_candidate AS c ON c.content_hash = o.content_hash
    WHERE r.album_id IN (
          SELECT credit.album_id FROM album_artists AS credit
          WHERE credit.artist_id = NEW.artist_id AND credit.credit_key = NEW.name_key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_artist_credit_removed AFTER DELETE ON artist_credits
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM import_candidate_album_artist_assignment AS a
    JOIN scan_candidate AS c ON c.content_hash = a.content_hash
    WHERE a.artist_id = OLD.artist_id AND a.credit_key = OLD.name_key;
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM releases AS r
    JOIN local_release_origin AS o ON o.release_id = r.id
    JOIN scan_candidate AS c ON c.content_hash = o.content_hash
    WHERE r.album_id IN (
          SELECT credit.album_id FROM album_artists AS credit
          WHERE credit.artist_id = OLD.artist_id AND credit.credit_key = OLD.name_key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_artist_credit_changed
AFTER UPDATE OF artist_id, name_key, name ON artist_credits
WHEN (OLD.artist_id, OLD.name_key, OLD.name) IS NOT (NEW.artist_id, NEW.name_key, NEW.name)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM import_candidate_album_artist_assignment AS a
    JOIN scan_candidate AS c ON c.content_hash = a.content_hash
    WHERE (a.artist_id = OLD.artist_id AND a.credit_key = OLD.name_key)
       OR (a.artist_id = NEW.artist_id AND a.credit_key = NEW.name_key);
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM releases AS r
    JOIN local_release_origin AS o ON o.release_id = r.id
    JOIN scan_candidate AS c ON c.content_hash = o.content_hash
    WHERE r.album_id IN (
          SELECT credit.album_id FROM album_artists AS credit
          WHERE (credit.artist_id = OLD.artist_id AND credit.credit_key = OLD.name_key)
             OR (credit.artist_id = NEW.artist_id AND credit.credit_key = NEW.name_key));
END;

-- What an imported release's Done row shows of its library album: the
-- album's title, its credits and the names they print. The Done rows of an
-- album are those of its releases this device imported.
CREATE TRIGGER IF NOT EXISTS import_list_on_album_retitled AFTER UPDATE OF title ON albums
WHEN OLD.title IS NOT NEW.title
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM releases AS r
    JOIN local_release_origin AS o ON o.release_id = r.id
    JOIN scan_candidate AS c ON c.content_hash = o.content_hash
    WHERE r.album_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_release_moved AFTER UPDATE OF album_id ON releases
WHEN OLD.album_id IS NOT NEW.album_id
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM releases AS r
    JOIN local_release_origin AS o ON o.release_id = r.id
    JOIN scan_candidate AS c ON c.content_hash = o.content_hash
    WHERE r.id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_album_credit_added AFTER INSERT ON album_artists
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM releases AS r
    JOIN local_release_origin AS o ON o.release_id = r.id
    JOIN scan_candidate AS c ON c.content_hash = o.content_hash
    WHERE r.album_id = NEW.album_id;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_album_credit_removed AFTER DELETE ON album_artists
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM releases AS r
    JOIN local_release_origin AS o ON o.release_id = r.id
    JOIN scan_candidate AS c ON c.content_hash = o.content_hash
    WHERE r.album_id = OLD.album_id;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_album_credit_changed
AFTER UPDATE OF album_id, artist_id, position, credit_key, join_phrase ON album_artists
WHEN (OLD.album_id, OLD.artist_id, OLD.position, OLD.credit_key, OLD.join_phrase)
  IS NOT (NEW.album_id, NEW.artist_id, NEW.position, NEW.credit_key, NEW.join_phrase)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM releases AS r
    JOIN local_release_origin AS o ON o.release_id = r.id
    JOIN scan_candidate AS c ON c.content_hash = o.content_hash
    WHERE r.album_id IN (OLD.album_id, NEW.album_id);
END;

-- A merge changing the artist a credit shows as moves the credits that print
-- it.
CREATE TRIGGER IF NOT EXISTS import_list_on_artist_shown_changed
AFTER UPDATE OF shown_artist_id ON artist_shown_as
WHEN OLD.shown_artist_id IS NOT NEW.shown_artist_id
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT c.path FROM releases AS r
    JOIN local_release_origin AS o ON o.release_id = r.id
    JOIN scan_candidate AS c ON c.content_hash = o.content_hash
    WHERE r.album_id IN (SELECT album_id FROM album_artists WHERE artist_id = NEW.artist_id);
END;

