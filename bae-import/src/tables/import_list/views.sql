-- Each import list a surface has open, as it is viewed: its tab, filter
-- entry and filter text, and its order; which of its groups are folded shut
-- (`import_list_collapsed`); and the rows selected in it
-- (`import_list_selection`). What a list shows is a query over these and the
-- placement, so a change to any of them is a write the list's live query
-- answers. The revisions count changes to the view, its folds included, and
-- to the selection, so a surface can wait for the list value that reflects
-- one. No list outlives the process that opened it: the import service
-- empties these tables as the library opens.
CREATE TABLE IF NOT EXISTS import_list (
    id                 INTEGER PRIMARY KEY,
    tab                TEXT NOT NULL CHECK (tab IN ('pending', 'done', 'skipped')),
    pending_filter     TEXT NOT NULL CHECK (pending_filter IN (
        'all', 'needs_you', 'in_progress', 'identified', 'unmatched', 'not_looked_up')),
    filter_text        TEXT NOT NULL,
    -- The `GLOB` pattern a row's shown text, case folded as `filter_title`
    -- is, must match to be kept: the filter text anywhere inside it, as
    -- library search matches a query. NULL with no filter text, which keeps
    -- every row.
    pattern            TEXT GENERATED ALWAYS AS (
        CASE WHEN filter_text <> '' THEN bae_search_pattern(filter_text) END) STORED,
    list_order         TEXT NOT NULL CHECK (list_order IN (
        'newest_first', 'oldest_first', 'path_ascending', 'path_descending')),
    view_revision      INTEGER NOT NULL DEFAULT 0 CHECK (view_revision >= 0),
    selection_revision INTEGER NOT NULL DEFAULT 0 CHECK (selection_revision >= 0),
    -- Where the list's view opened: the position its first selected row had
    -- in it when the view was set, or its top while it showed no selected
    -- row. The write that sets the view writes it; nothing else moves it.
    opened_at          INTEGER NOT NULL DEFAULT 0 CHECK (opened_at >= 0)
) STRICT;

-- The groups a list shows folded shut: their members are not in it. A group
-- that is gone takes its folded state with it.
CREATE TABLE IF NOT EXISTS import_list_collapsed (
    list_id             INTEGER NOT NULL REFERENCES import_list (id) ON DELETE CASCADE,
    watched_folder_path BLOB NOT NULL,
    item                TEXT NOT NULL DEFAULT 'group' CHECK (item = 'group'),
    group_folder        BLOB NOT NULL,
    PRIMARY KEY (list_id, watched_folder_path, group_folder),
    FOREIGN KEY (watched_folder_path, item, group_folder)
        REFERENCES import_list_placement (watched_folder_path, item, key) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS import_list_collapsed_by_group
    ON import_list_collapsed (watched_folder_path, item, group_folder);

-- The rows selected in a list, by key: only ever rows it shows. A row the
-- list stops showing, by a change to its view or to the row, leaves the
-- selection in that write.
CREATE TABLE IF NOT EXISTS import_list_selection (
    list_id INTEGER NOT NULL REFERENCES import_list (id) ON DELETE CASCADE,
    key     BLOB NOT NULL,
    PRIMARY KEY (list_id, key)
) STRICT;

CREATE INDEX IF NOT EXISTS import_list_selection_by_key ON import_list_selection (key);

-- Each list's view as the queries below read it: whether its filters
-- narrow its tab, by filter text or by a filter entry on Found, whether it
-- folds any group shut, and whether it keeps every row of its tab, its
-- filters narrowing nothing and no group folded shut.
CREATE VIEW IF NOT EXISTS import_list_view AS
SELECT l.*, NOT l.narrows AND NOT l.folds AS keeps_all
FROM (SELECT *, pattern IS NOT NULL OR (tab = 'pending' AND pending_filter <> 'all') AS narrows,
             EXISTS (SELECT 1 FROM import_list_collapsed AS c WHERE c.list_id = import_list.id)
                 AS folds
      FROM import_list) AS l;

-- Each list's rows of its tab, each with whether its filters keep it: on
-- Found, in its filter entry; and with its shown text matching its search's
-- pattern, the test `import_list_search` finds the matching rows by. And
-- whether the list folds the row's group shut. The tab is matched by value
-- rather than by index (`+p.tab`), so a lookup of a group's members reaches
-- them by their group.
CREATE VIEW IF NOT EXISTS import_list_judged AS
SELECT v.id AS list_id, v.keeps_all, p.*,
       (v.tab <> 'pending' OR v.pending_filter = 'all' OR p.pending_entry = v.pending_filter)
       AND (v.pattern IS NULL OR p.filter_title GLOB v.pattern
            OR p.filter_artists GLOB v.pattern) AS kept,
       p.group_folder IS NOT NULL
       AND EXISTS (SELECT 1 FROM import_list_collapsed AS c
                   WHERE c.list_id = v.id AND c.watched_folder_path = p.watched_folder_path
                     AND c.group_folder = p.group_folder) AS folded
FROM import_list_view AS v
JOIN import_list_placement AS p ON +p.tab = v.tab;

-- Each searching list's rows whose shown text matches its search's pattern,
-- every tab's, as the search index finds them: what a search's counts and
-- windows read instead of testing every row of the tab.
CREATE VIEW IF NOT EXISTS import_list_found AS
SELECT l.id AS list_id, s.rowid AS entry
FROM import_list AS l
JOIN import_list_search AS s ON s.title GLOB l.pattern OR s.artists GLOB l.pattern;

-- Each list's releases its filters keep, folded groups' members included.
CREATE VIEW IF NOT EXISTS import_list_matching AS
SELECT * FROM import_list_judged WHERE item <> 'group' AND kept;

-- Each list's items, in no order: the releases its filters keep outside the
-- groups it folds shut, and the header of each group holding a release its
-- filters keep.
CREATE VIEW IF NOT EXISTS import_list_item AS
SELECT * FROM import_list_judged AS j
WHERE j.keeps_all
   OR CASE WHEN j.item = 'group'
           THEN EXISTS (SELECT 1 FROM import_list_matching AS m
                        WHERE m.list_id = j.list_id
                          AND m.watched_folder_path = j.watched_folder_path
                          AND m.group_folder = j.key)
           ELSE j.kept AND NOT j.folded END;

-- Each searching list's items, read from its search's matches rather than
-- from every row of its tab: the matched releases among its items, and the
-- headers among them of the groups holding a release its filters keep. The
-- same items `import_list_item` holds for the list. Each row is reached
-- from its match (`CROSS JOIN` keeps that order), never the tab scanned for
-- its matches.
CREATE VIEW IF NOT EXISTS import_list_found_item AS
SELECT i.* FROM import_list_found AS f
CROSS JOIN import_list_item AS i ON i.list_id = f.list_id AND i.entry = f.entry
UNION ALL
SELECT h.*
FROM (SELECT DISTINCT m.list_id, m.watched_folder_path, m.group_folder
      FROM import_list_found AS f
      CROSS JOIN import_list_matching AS m ON m.list_id = f.list_id AND m.entry = f.entry
      WHERE m.group_folder IS NOT NULL) AS g
CROSS JOIN import_list_item AS h
    ON h.list_id = g.list_id AND h.watched_folder_path = g.watched_folder_path
   AND h.item = 'group' AND h.key = g.group_folder;

-- Each list's releases it shows.
CREATE VIEW IF NOT EXISTS import_list_shown AS
SELECT * FROM import_list_item WHERE item <> 'group';

-- A selected release a change moves out of what a list shows leaves its
-- selection; its selection's revision moves with it.
CREATE TRIGGER IF NOT EXISTS import_list_selected_moved AFTER UPDATE ON import_list_placement
WHEN NEW.item <> 'group'
 AND (OLD.tab, OLD.pending_state, OLD.filter_title, OLD.filter_artists, OLD.group_folder)
     IS NOT (NEW.tab, NEW.pending_state, NEW.filter_title, NEW.filter_artists, NEW.group_folder)
BEGIN
    DELETE FROM import_list_selection
    WHERE key = NEW.key
      AND NOT EXISTS (SELECT 1 FROM import_list_shown AS shown
                      WHERE shown.list_id = import_list_selection.list_id
                        AND shown.key = NEW.key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_selected_removed AFTER DELETE ON import_list_placement
WHEN OLD.item <> 'group'
BEGIN
    DELETE FROM import_list_selection
    WHERE key = OLD.key
      AND NOT EXISTS (SELECT 1 FROM import_list_shown AS shown
                      WHERE shown.list_id = import_list_selection.list_id
                        AND shown.key = OLD.key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_view_changed
AFTER UPDATE OF tab, pending_filter, filter_text ON import_list
WHEN (OLD.tab, OLD.pending_filter, OLD.filter_text)
     IS NOT (NEW.tab, NEW.pending_filter, NEW.filter_text)
BEGIN
    DELETE FROM import_list_selection
    WHERE list_id = NEW.id
      AND NOT EXISTS (SELECT 1 FROM import_list_shown AS shown
                      WHERE shown.list_id = NEW.id AND shown.key = import_list_selection.key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_group_folded AFTER INSERT ON import_list_collapsed
BEGIN
    DELETE FROM import_list_selection
    WHERE list_id = NEW.list_id
      AND key IN (SELECT key FROM import_list_placement
                  WHERE watched_folder_path = NEW.watched_folder_path
                    AND group_folder = NEW.group_folder AND item <> 'group');
END;

-- Every change to a selection, the person's or one that hid a row, moves its
-- revision.
CREATE TRIGGER IF NOT EXISTS import_list_selection_added AFTER INSERT ON import_list_selection
BEGIN
    UPDATE import_list SET selection_revision = selection_revision + 1 WHERE id = NEW.list_id;
END;

CREATE TRIGGER IF NOT EXISTS import_list_selection_removed AFTER DELETE ON import_list_selection
BEGIN
    UPDATE import_list SET selection_revision = selection_revision + 1 WHERE id = OLD.list_id;
END;
