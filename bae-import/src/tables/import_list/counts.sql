-- How many placement rows each tab holds of each item and, on Found, in each
-- of its filter's entries: what the tab bar, Found's filter and the list's
-- count read, so none of them counts the placement. One row per tab, item and
-- entry that holds a placement row, which is nine rows at most. A row stands
-- while its count is above zero. The triggers below keep it in the write that
-- places, moves or removes a placement row. Computed from device-local rows
-- and never synced.
CREATE TABLE IF NOT EXISTS import_list_count (
    tab           TEXT NOT NULL,
    item          TEXT NOT NULL,
    -- The entry of Found's filter holding the rows; NULL off Found's
    -- releases.
    pending_entry TEXT,
    count         INTEGER NOT NULL CHECK (count > 0)
) STRICT;

-- A row with no entry is keyed like any other, so it has one row too.
CREATE UNIQUE INDEX IF NOT EXISTS import_list_count_key
    ON import_list_count (tab, item, ifnull(pending_entry, ''));

-- Count one placement row in, or out. Procedure entry points that store
-- nothing, like `import_list_replace`. Counting out a row no count holds
-- fails the write.
CREATE VIEW IF NOT EXISTS import_list_count_in AS
SELECT tab, item, pending_entry FROM import_list_placement WHERE 0;

CREATE VIEW IF NOT EXISTS import_list_count_out AS
SELECT tab, item, pending_entry FROM import_list_placement WHERE 0;

CREATE TRIGGER IF NOT EXISTS import_list_count_in_row INSTEAD OF INSERT ON import_list_count_in
BEGIN
    INSERT INTO import_list_count (tab, item, pending_entry, count)
    VALUES (NEW.tab, NEW.item, NEW.pending_entry, 1)
    ON CONFLICT (tab, item, ifnull(pending_entry, '')) DO UPDATE SET count = count + 1;
END;

CREATE TRIGGER IF NOT EXISTS import_list_count_out_row INSTEAD OF INSERT ON import_list_count_out
BEGIN
    SELECT RAISE(ABORT, 'a placement row counted out was never counted in')
    WHERE NOT EXISTS (SELECT 1 FROM import_list_count
                      WHERE tab = NEW.tab AND item = NEW.item
                        AND ifnull(pending_entry, '') = ifnull(NEW.pending_entry, ''));
    DELETE FROM import_list_count
    WHERE tab = NEW.tab AND item = NEW.item
      AND ifnull(pending_entry, '') = ifnull(NEW.pending_entry, '') AND count = 1;
    UPDATE import_list_count SET count = count - 1
    WHERE tab = NEW.tab AND item = NEW.item
      AND ifnull(pending_entry, '') = ifnull(NEW.pending_entry, '');
END;

CREATE TRIGGER IF NOT EXISTS import_list_counted_in AFTER INSERT ON import_list_placement
BEGIN
    INSERT INTO import_list_count_in (tab, item, pending_entry)
    VALUES (NEW.tab, NEW.item, NEW.pending_entry);
END;

CREATE TRIGGER IF NOT EXISTS import_list_counted_out AFTER DELETE ON import_list_placement
BEGIN
    INSERT INTO import_list_count_out (tab, item, pending_entry)
    VALUES (OLD.tab, OLD.item, OLD.pending_entry);
END;

-- `pending_entry` is computed from `pending_state`, so a change to the state
-- is what moves a row between entries.
CREATE TRIGGER IF NOT EXISTS import_list_counted_again
AFTER UPDATE OF tab, item, pending_state ON import_list_placement
WHEN (OLD.tab, OLD.item, OLD.pending_entry) IS NOT (NEW.tab, NEW.item, NEW.pending_entry)
BEGIN
    INSERT INTO import_list_count_out (tab, item, pending_entry)
    VALUES (OLD.tab, OLD.item, OLD.pending_entry);
    INSERT INTO import_list_count_in (tab, item, pending_entry)
    VALUES (NEW.tab, NEW.item, NEW.pending_entry);
END;

-- How many of each Found group's members each entry of Found's filter holds:
-- what Found's count reads for the members of the groups it folds shut and,
-- under an entry, for the headers of the groups holding the entry's releases,
-- so neither walks the members. One row per group and entry holding a
-- member. A row stands while its count is above zero. The triggers below
-- keep it in the write that places, moves or removes a member. Computed from
-- device-local rows and never synced.
CREATE TABLE IF NOT EXISTS import_list_group_count (
    watched_folder_path BLOB NOT NULL,
    -- The group's folder, as the placement's `group_folder` holds it.
    group_folder        BLOB NOT NULL,
    pending_entry       TEXT NOT NULL,
    count               INTEGER NOT NULL CHECK (count > 0),
    PRIMARY KEY (watched_folder_path, group_folder, pending_entry)
) STRICT, WITHOUT ROWID;

-- The groups holding an entry's releases, with their counts.
CREATE INDEX IF NOT EXISTS import_list_group_count_by_entry
    ON import_list_group_count (pending_entry, count);

-- Count one member in, or out, like `import_list_count_in` and
-- `import_list_count_out`.
CREATE VIEW IF NOT EXISTS import_list_group_count_in AS
SELECT watched_folder_path, group_folder, pending_entry FROM import_list_placement WHERE 0;

CREATE VIEW IF NOT EXISTS import_list_group_count_out AS
SELECT watched_folder_path, group_folder, pending_entry FROM import_list_placement WHERE 0;

CREATE TRIGGER IF NOT EXISTS import_list_group_count_in_row
INSTEAD OF INSERT ON import_list_group_count_in
BEGIN
    INSERT INTO import_list_group_count (watched_folder_path, group_folder, pending_entry, count)
    VALUES (NEW.watched_folder_path, NEW.group_folder, NEW.pending_entry, 1)
    ON CONFLICT (watched_folder_path, group_folder, pending_entry)
    DO UPDATE SET count = count + 1;
END;

CREATE TRIGGER IF NOT EXISTS import_list_group_count_out_row
INSTEAD OF INSERT ON import_list_group_count_out
BEGIN
    SELECT RAISE(ABORT, 'a member counted out was never counted in')
    WHERE NOT EXISTS (SELECT 1 FROM import_list_group_count
                      WHERE watched_folder_path = NEW.watched_folder_path
                        AND group_folder = NEW.group_folder
                        AND pending_entry = NEW.pending_entry);
    DELETE FROM import_list_group_count
    WHERE watched_folder_path = NEW.watched_folder_path AND group_folder = NEW.group_folder
      AND pending_entry = NEW.pending_entry AND count = 1;
    UPDATE import_list_group_count SET count = count - 1
    WHERE watched_folder_path = NEW.watched_folder_path AND group_folder = NEW.group_folder
      AND pending_entry = NEW.pending_entry;
END;

-- A member is a release with a group: only Found's releases have one, and
-- each of them is in an entry.
CREATE TRIGGER IF NOT EXISTS import_list_member_counted_in AFTER INSERT ON import_list_placement
WHEN NEW.item <> 'group' AND NEW.group_folder IS NOT NULL
BEGIN
    INSERT INTO import_list_group_count_in (watched_folder_path, group_folder, pending_entry)
    VALUES (NEW.watched_folder_path, NEW.group_folder, NEW.pending_entry);
END;

CREATE TRIGGER IF NOT EXISTS import_list_member_counted_out AFTER DELETE ON import_list_placement
WHEN OLD.item <> 'group' AND OLD.group_folder IS NOT NULL
BEGIN
    INSERT INTO import_list_group_count_out (watched_folder_path, group_folder, pending_entry)
    VALUES (OLD.watched_folder_path, OLD.group_folder, OLD.pending_entry);
END;

-- `group_folder` is computed from the tab, the item, the folder and whether
-- the release holds others below it, and `pending_entry` from the state, so a
-- change to one of those is what moves a member between groups and entries,
-- or makes a release a member or no longer one.
CREATE TRIGGER IF NOT EXISTS import_list_member_counted_again
AFTER UPDATE OF watched_folder_path, item, tab, pending_state, folder, holds_nested
ON import_list_placement
WHEN ((OLD.item <> 'group' AND OLD.group_folder IS NOT NULL)
      OR (NEW.item <> 'group' AND NEW.group_folder IS NOT NULL))
 AND (OLD.watched_folder_path, OLD.item, OLD.group_folder, OLD.pending_entry)
     IS NOT (NEW.watched_folder_path, NEW.item, NEW.group_folder, NEW.pending_entry)
BEGIN
    INSERT INTO import_list_group_count_out (watched_folder_path, group_folder, pending_entry)
    SELECT OLD.watched_folder_path, OLD.group_folder, OLD.pending_entry
    WHERE OLD.item <> 'group' AND OLD.group_folder IS NOT NULL;
    INSERT INTO import_list_group_count_in (watched_folder_path, group_folder, pending_entry)
    SELECT NEW.watched_folder_path, NEW.group_folder, NEW.pending_entry
    WHERE NEW.item <> 'group' AND NEW.group_folder IS NOT NULL;
END;
