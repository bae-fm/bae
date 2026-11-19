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
