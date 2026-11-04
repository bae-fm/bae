-- The list's search (§3.2) finds the rows whose shown text holds what was
-- typed the way library search finds its titles: each release's
-- shown title and artists, folded as `filter_title` and `filter_artists` hold
-- them, in an FTS5 trigram index under its placement row's number, so a
-- search of three characters or more reads the rows it finds and not the
-- whole queue. The triggers below keep it in the write that places, changes
-- or removes the row. A header shows no text and has no row here.
CREATE VIRTUAL TABLE IF NOT EXISTS import_list_search USING fts5(
    title, artists,
    tokenize = 'trigram case_sensitive 1'
);

CREATE TRIGGER IF NOT EXISTS import_list_search_added AFTER INSERT ON import_list_placement
WHEN NEW.item <> 'group'
BEGIN
    INSERT INTO import_list_search (rowid, title, artists)
    VALUES (NEW.entry, NEW.filter_title, NEW.filter_artists);
END;

CREATE TRIGGER IF NOT EXISTS import_list_search_changed
AFTER UPDATE OF shown_title, shown_artists ON import_list_placement
WHEN NEW.item <> 'group'
 AND (OLD.filter_title, OLD.filter_artists) IS NOT (NEW.filter_title, NEW.filter_artists)
BEGIN
    UPDATE import_list_search SET title = NEW.filter_title, artists = NEW.filter_artists
    WHERE rowid = NEW.entry;
END;

CREATE TRIGGER IF NOT EXISTS import_list_search_removed AFTER DELETE ON import_list_placement
WHEN OLD.item <> 'group'
BEGIN
    DELETE FROM import_list_search WHERE rowid = OLD.entry;
END;
