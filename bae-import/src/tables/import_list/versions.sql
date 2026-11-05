-- How many times the rows of each table the import list's order reads have
-- moved: a row inserted or deleted, or one of the values the list sorts or
-- filters it by changed. The placement, and the folders' hashes
-- (`scan_candidate`) and the releases this device imported from them
-- (`local_release_origin`), by which Done finds the releases still uploading,
-- by their place in the upload queue (`release_storage`).
-- Triggers on each table advance its version in the write that moves its
-- rows. A list's read starts a window from a row an earlier read found only
-- while these versions stand where they stood for that read (§9.2).
CREATE TABLE IF NOT EXISTS import_list_versions (
    source  TEXT PRIMARY KEY CHECK (source IN (
        'import_list_placement', 'local_release_origin', 'release_storage', 'scan_candidate')),
    version INTEGER NOT NULL
) STRICT;

INSERT OR IGNORE INTO import_list_versions (source, version) VALUES
    ('import_list_placement', 0), ('local_release_origin', 0), ('release_storage', 0),
    ('scan_candidate', 0);

CREATE TRIGGER IF NOT EXISTS import_list_placement_order_added AFTER INSERT ON import_list_placement
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'import_list_placement';
END;

CREATE TRIGGER IF NOT EXISTS import_list_placement_order_removed AFTER DELETE ON import_list_placement
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'import_list_placement';
END;

CREATE TRIGGER IF NOT EXISTS import_list_placement_order_moved AFTER UPDATE OF watched_folder_path, item, key, root_position, tab, pending_state, holds_nested, own_date, group_date, folder, shown_title, shown_artists ON import_list_placement
WHEN OLD.watched_folder_path IS NOT NEW.watched_folder_path
  OR OLD.item IS NOT NEW.item
  OR OLD.key IS NOT NEW.key
  OR OLD.root_position IS NOT NEW.root_position
  OR OLD.tab IS NOT NEW.tab
  OR OLD.pending_entry IS NOT NEW.pending_entry
  OR OLD.group_folder IS NOT NEW.group_folder
  OR OLD.own_date IS NOT NEW.own_date
  OR OLD.unit_date IS NOT NEW.unit_date
  OR OLD.unit_path IS NOT NEW.unit_path
  OR OLD.folder IS NOT NEW.folder
  OR OLD.filter_title IS NOT NEW.filter_title
  OR OLD.filter_artists IS NOT NEW.filter_artists
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'import_list_placement';
END;

CREATE TRIGGER IF NOT EXISTS local_release_origin_order_added AFTER INSERT ON local_release_origin
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'local_release_origin';
END;

CREATE TRIGGER IF NOT EXISTS local_release_origin_order_removed AFTER DELETE ON local_release_origin
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'local_release_origin';
END;

CREATE TRIGGER IF NOT EXISTS local_release_origin_order_moved AFTER UPDATE OF release_id, content_hash ON local_release_origin
WHEN OLD.release_id IS NOT NEW.release_id
  OR OLD.content_hash IS NOT NEW.content_hash
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'local_release_origin';
END;

CREATE TRIGGER IF NOT EXISTS release_storage_order_added AFTER INSERT ON release_storage
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'release_storage';
END;

CREATE TRIGGER IF NOT EXISTS release_storage_order_removed AFTER DELETE ON release_storage
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'release_storage';
END;

CREATE TRIGGER IF NOT EXISTS release_storage_order_moved AFTER UPDATE OF first_queued_position ON release_storage
WHEN OLD.first_queued_position IS NOT NEW.first_queued_position
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'release_storage';
END;

CREATE TRIGGER IF NOT EXISTS scan_candidate_order_added AFTER INSERT ON scan_candidate
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'scan_candidate';
END;

CREATE TRIGGER IF NOT EXISTS scan_candidate_order_removed AFTER DELETE ON scan_candidate
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'scan_candidate';
END;

CREATE TRIGGER IF NOT EXISTS scan_candidate_order_moved AFTER UPDATE OF path, content_hash ON scan_candidate
WHEN OLD.path IS NOT NEW.path
  OR OLD.content_hash IS NOT NEW.content_hash
BEGIN
    UPDATE import_list_versions SET version = version + 1 WHERE source = 'scan_candidate';
END;
