-- Library search (§2.3) matches a query anywhere inside a title or a name,
-- ignoring case: each searched text is kept in an FTS5 table as
-- `bae_fold_case` folds it, which is how the query is folded too, indexed by
-- its runs of three characters, so a query of three or more finds its rows
-- without reading every title; a shorter one reads the whole table. Each FTS5
-- row also carries, unindexed, the key of the row it searches and what that
-- kind of result is ordered by, so a search orders its matches without
-- reading the rows they stand for. An FTS5 row is found by an integer, which
-- each `_entry` table pairs with the searched row's key for the triggers that
-- keep it: SQLite's own rowid of a synced row is no part of it.

-- Album titles.
CREATE TABLE IF NOT EXISTS album_search_entry (
    entry INTEGER PRIMARY KEY,
    album_id TEXT NOT NULL,
    UNIQUE (album_id)
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS album_search USING fts5(
    title, album_id UNINDEXED,
    tokenize = 'trigram case_sensitive 1'
);

CREATE TRIGGER IF NOT EXISTS album_search_added AFTER INSERT ON albums
BEGIN
    INSERT INTO album_search_entry (album_id) VALUES (NEW.id);
    INSERT INTO album_search (rowid, title, album_id)
    VALUES ((SELECT entry FROM album_search_entry WHERE album_id = NEW.id), bae_fold_case(NEW.title), NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS album_search_changed AFTER UPDATE OF title ON albums
BEGIN
    UPDATE album_search SET title = bae_fold_case(NEW.title) WHERE rowid = (SELECT entry FROM album_search_entry WHERE album_id = NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS album_search_removed AFTER DELETE ON albums
BEGIN
    DELETE FROM album_search WHERE rowid = (SELECT entry FROM album_search_entry WHERE album_id = OLD.id);
    DELETE FROM album_search_entry WHERE album_id = OLD.id;
END;

-- Track titles.
CREATE TABLE IF NOT EXISTS track_search_entry (
    entry INTEGER PRIMARY KEY,
    track_id TEXT NOT NULL,
    UNIQUE (track_id)
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS track_search USING fts5(
    title, track_id UNINDEXED,
    tokenize = 'trigram case_sensitive 1'
);

CREATE TRIGGER IF NOT EXISTS track_search_added AFTER INSERT ON tracks
BEGIN
    INSERT INTO track_search_entry (track_id) VALUES (NEW.id);
    INSERT INTO track_search (rowid, title, track_id)
    VALUES ((SELECT entry FROM track_search_entry WHERE track_id = NEW.id), bae_fold_case(NEW.title), NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS track_search_changed AFTER UPDATE OF title ON tracks
BEGIN
    UPDATE track_search SET title = bae_fold_case(NEW.title) WHERE rowid = (SELECT entry FROM track_search_entry WHERE track_id = NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS track_search_removed AFTER DELETE ON tracks
BEGIN
    DELETE FROM track_search WHERE rowid = (SELECT entry FROM track_search_entry WHERE track_id = OLD.id);
    DELETE FROM track_search_entry WHERE track_id = OLD.id;
END;

-- Work titles.
CREATE TABLE IF NOT EXISTS work_search_entry (
    entry INTEGER PRIMARY KEY,
    work_id TEXT NOT NULL,
    UNIQUE (work_id)
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS work_search USING fts5(
    title, work_id UNINDEXED,
    tokenize = 'trigram case_sensitive 1'
);

CREATE TRIGGER IF NOT EXISTS work_search_added AFTER INSERT ON works
BEGIN
    INSERT INTO work_search_entry (work_id) VALUES (NEW.id);
    INSERT INTO work_search (rowid, title, work_id)
    VALUES ((SELECT entry FROM work_search_entry WHERE work_id = NEW.id), bae_fold_case(NEW.title), NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS work_search_changed AFTER UPDATE OF title ON works
BEGIN
    UPDATE work_search SET title = bae_fold_case(NEW.title) WHERE rowid = (SELECT entry FROM work_search_entry WHERE work_id = NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS work_search_removed AFTER DELETE ON works
BEGIN
    DELETE FROM work_search WHERE rowid = (SELECT entry FROM work_search_entry WHERE work_id = OLD.id);
    DELETE FROM work_search_entry WHERE work_id = OLD.id;
END;

-- Artists' names and sort names.
CREATE TABLE IF NOT EXISTS artist_search_entry (
    entry INTEGER PRIMARY KEY,
    artist_id TEXT NOT NULL,
    UNIQUE (artist_id)
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS artist_search USING fts5(
    name, sort_name, artist_id UNINDEXED,
    tokenize = 'trigram case_sensitive 1'
);

CREATE TRIGGER IF NOT EXISTS artist_search_added AFTER INSERT ON artists
BEGIN
    INSERT INTO artist_search_entry (artist_id) VALUES (NEW.id);
    INSERT INTO artist_search (rowid, name, sort_name, artist_id)
    VALUES ((SELECT entry FROM artist_search_entry WHERE artist_id = NEW.id), bae_fold_case(NEW.name), bae_fold_case(NEW.sort_name), NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS artist_search_changed AFTER UPDATE OF name, sort_name ON artists
BEGIN
    UPDATE artist_search SET name = bae_fold_case(NEW.name), sort_name = bae_fold_case(NEW.sort_name) WHERE rowid = (SELECT entry FROM artist_search_entry WHERE artist_id = NEW.id);
END;

CREATE TRIGGER IF NOT EXISTS artist_search_removed AFTER DELETE ON artists
BEGIN
    DELETE FROM artist_search WHERE rowid = (SELECT entry FROM artist_search_entry WHERE artist_id = OLD.id);
    DELETE FROM artist_search_entry WHERE artist_id = OLD.id;
END;

-- Artists' other printed names.
CREATE TABLE IF NOT EXISTS artist_credit_search_entry (
    entry INTEGER PRIMARY KEY,
    artist_id TEXT NOT NULL,
    name_key TEXT NOT NULL,
    UNIQUE (artist_id, name_key)
) STRICT;

CREATE VIRTUAL TABLE IF NOT EXISTS artist_credit_search USING fts5(
    name, artist_id UNINDEXED, name_key UNINDEXED,
    tokenize = 'trigram case_sensitive 1'
);

CREATE TRIGGER IF NOT EXISTS artist_credit_search_added AFTER INSERT ON artist_credits
BEGIN
    INSERT INTO artist_credit_search_entry (artist_id, name_key) VALUES (NEW.artist_id, NEW.name_key);
    INSERT INTO artist_credit_search (rowid, name, artist_id, name_key)
    VALUES ((SELECT entry FROM artist_credit_search_entry WHERE artist_id = NEW.artist_id AND name_key = NEW.name_key), bae_fold_case(NEW.name), NEW.artist_id, NEW.name_key);
END;

CREATE TRIGGER IF NOT EXISTS artist_credit_search_changed AFTER UPDATE OF name ON artist_credits
BEGIN
    UPDATE artist_credit_search SET name = bae_fold_case(NEW.name) WHERE rowid = (SELECT entry FROM artist_credit_search_entry WHERE artist_id = NEW.artist_id AND name_key = NEW.name_key);
END;

CREATE TRIGGER IF NOT EXISTS artist_credit_search_removed AFTER DELETE ON artist_credits
BEGIN
    DELETE FROM artist_credit_search WHERE rowid = (SELECT entry FROM artist_credit_search_entry WHERE artist_id = OLD.artist_id AND name_key = OLD.name_key);
    DELETE FROM artist_credit_search_entry WHERE artist_id = OLD.artist_id AND name_key = OLD.name_key;
END;
