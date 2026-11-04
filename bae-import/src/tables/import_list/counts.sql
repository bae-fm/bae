-- How many placement rows each tab holds of each item; how many of Found's
-- releases each of its filter entries holds; and how many of each group's
-- releases each entry holds. What the list's counts read, so none counts the
-- placement. A row stands while its count is above zero. The triggers below
-- keep them in the write that moves a placement row.
CREATE TABLE IF NOT EXISTS import_list_tab_count (
    tab   TEXT NOT NULL,
    item  TEXT NOT NULL,
    count INTEGER NOT NULL CHECK (count > 0),
    PRIMARY KEY (tab, item)
) STRICT;

CREATE TABLE IF NOT EXISTS import_list_entry_count (
    pending_entry TEXT PRIMARY KEY,
    count         INTEGER NOT NULL CHECK (count > 0)
) STRICT;

CREATE TABLE IF NOT EXISTS import_list_group_entry_count (
    watched_folder_path BLOB NOT NULL,
    group_folder        BLOB NOT NULL,
    pending_entry       TEXT NOT NULL,
    count               INTEGER NOT NULL CHECK (count > 0),
    PRIMARY KEY (watched_folder_path, group_folder, pending_entry)
) STRICT;

-- Count a placement row in, or out. Procedure entry points that store
-- nothing, like `import_list_replace`. Counting out a row no count holds
-- fails the write.
CREATE VIEW IF NOT EXISTS import_list_count_in AS
SELECT watched_folder_path, item, tab, pending_entry, group_folder
FROM import_list_placement WHERE 0;

CREATE VIEW IF NOT EXISTS import_list_count_out AS
SELECT watched_folder_path, item, tab, pending_entry, group_folder
FROM import_list_placement WHERE 0;

CREATE TRIGGER IF NOT EXISTS import_list_count_in_row INSTEAD OF INSERT ON import_list_count_in
BEGIN
    INSERT INTO import_list_tab_count (tab, item, count) VALUES (NEW.tab, NEW.item, 1)
    ON CONFLICT (tab, item) DO UPDATE SET count = count + 1;
    INSERT INTO import_list_entry_count (pending_entry, count)
    SELECT NEW.pending_entry, 1 WHERE NEW.pending_entry IS NOT NULL
    ON CONFLICT (pending_entry) DO UPDATE SET count = count + 1;
    INSERT INTO import_list_group_entry_count
        (watched_folder_path, group_folder, pending_entry, count)
    SELECT NEW.watched_folder_path, NEW.group_folder, NEW.pending_entry, 1
    WHERE NEW.item = 'candidate' AND NEW.group_folder IS NOT NULL
    ON CONFLICT (watched_folder_path, group_folder, pending_entry) DO UPDATE SET count = count + 1;
END;

CREATE TRIGGER IF NOT EXISTS import_list_count_out_row INSTEAD OF INSERT ON import_list_count_out
BEGIN
    SELECT RAISE(ABORT, 'a placement row counted out was never counted in')
    WHERE NOT EXISTS (SELECT 1 FROM import_list_tab_count
                      WHERE tab = NEW.tab AND item = NEW.item)
       OR (NEW.pending_entry IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM import_list_entry_count
                           WHERE pending_entry = NEW.pending_entry))
       OR (NEW.item = 'candidate' AND NEW.group_folder IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM import_list_group_entry_count
                           WHERE watched_folder_path = NEW.watched_folder_path
                             AND group_folder = NEW.group_folder
                             AND pending_entry = NEW.pending_entry));
    DELETE FROM import_list_tab_count WHERE tab = NEW.tab AND item = NEW.item AND count = 1;
    UPDATE import_list_tab_count SET count = count - 1 WHERE tab = NEW.tab AND item = NEW.item;
    DELETE FROM import_list_entry_count WHERE pending_entry = NEW.pending_entry AND count = 1;
    UPDATE import_list_entry_count SET count = count - 1 WHERE pending_entry = NEW.pending_entry;
    DELETE FROM import_list_group_entry_count
    WHERE NEW.item = 'candidate' AND watched_folder_path = NEW.watched_folder_path
      AND group_folder = NEW.group_folder AND pending_entry = NEW.pending_entry AND count = 1;
    UPDATE import_list_group_entry_count SET count = count - 1
    WHERE NEW.item = 'candidate' AND watched_folder_path = NEW.watched_folder_path
      AND group_folder = NEW.group_folder AND pending_entry = NEW.pending_entry;
END;

CREATE TRIGGER IF NOT EXISTS import_list_counted_in AFTER INSERT ON import_list_placement
BEGIN
    INSERT INTO import_list_count_in (watched_folder_path, item, tab, pending_entry, group_folder)
    VALUES (NEW.watched_folder_path, NEW.item, NEW.tab, NEW.pending_entry, NEW.group_folder);
END;

CREATE TRIGGER IF NOT EXISTS import_list_counted_out AFTER DELETE ON import_list_placement
BEGIN
    INSERT INTO import_list_count_out (watched_folder_path, item, tab, pending_entry, group_folder)
    VALUES (OLD.watched_folder_path, OLD.item, OLD.tab, OLD.pending_entry, OLD.group_folder);
END;

CREATE TRIGGER IF NOT EXISTS import_list_counted_again AFTER UPDATE ON import_list_placement
WHEN (OLD.watched_folder_path, OLD.item, OLD.tab, OLD.pending_entry, OLD.group_folder)
     IS NOT (NEW.watched_folder_path, NEW.item, NEW.tab, NEW.pending_entry, NEW.group_folder)
BEGIN
    INSERT INTO import_list_count_out (watched_folder_path, item, tab, pending_entry, group_folder)
    VALUES (OLD.watched_folder_path, OLD.item, OLD.tab, OLD.pending_entry, OLD.group_folder);
    INSERT INTO import_list_count_in (watched_folder_path, item, tab, pending_entry, group_folder)
    VALUES (NEW.watched_folder_path, NEW.item, NEW.tab, NEW.pending_entry, NEW.group_folder);
END;
