-- The placement follows what the scan stores: each change to its releases,
-- their folders' dates, the watched folders' order, the folders the person
-- skipped and a failed folder's tags places again the releases it can move
-- (`import_list_replace`). A change to a column the rules don't read places
-- nothing.

CREATE TRIGGER IF NOT EXISTS import_list_on_candidate_added AFTER INSERT ON scan_candidate
BEGIN
    INSERT INTO import_list_replace (key) VALUES (NEW.path);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_candidate_removed AFTER DELETE ON scan_candidate
BEGIN
    INSERT INTO import_list_replace (key) VALUES (OLD.path);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_candidate_changed AFTER UPDATE OF
    watched_folder_path, path, kind, name, relative_folder, folder, content_hash,
    file_edit_revision, grouping_key, source_kind ON scan_candidate
WHEN (OLD.watched_folder_path, OLD.path, OLD.kind, OLD.name, OLD.relative_folder, OLD.folder,
      OLD.content_hash, OLD.file_edit_revision, OLD.grouping_key, OLD.source_kind)
  IS NOT (NEW.watched_folder_path, NEW.path, NEW.kind, NEW.name, NEW.relative_folder, NEW.folder,
          NEW.content_hash, NEW.file_edit_revision, NEW.grouping_key, NEW.source_kind)
BEGIN
    INSERT INTO import_list_replace (key) SELECT OLD.path UNION SELECT NEW.path;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_watched_folder_moved
AFTER UPDATE OF position ON watched_import_folders
WHEN OLD.position IS NOT NEW.position
BEGIN
    UPDATE import_list_placement SET root_position = NEW.position
    WHERE watched_folder_path = NEW.path;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_discovery_added AFTER INSERT ON folder_discovery
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder = NEW.folder
    UNION SELECT grouping_key FROM release_grouping_member WHERE member_folder = NEW.folder;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_discovery_removed AFTER DELETE ON folder_discovery
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder = OLD.folder
    UNION SELECT grouping_key FROM release_grouping_member WHERE member_folder = OLD.folder;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_discovery_changed
AFTER UPDATE OF folder, first_seen_at, source_date ON folder_discovery
WHEN (OLD.folder, OLD.first_seen_at, OLD.source_date)
  IS NOT (NEW.folder, NEW.first_seen_at, NEW.source_date)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder IN (OLD.folder, NEW.folder)
    UNION SELECT grouping_key FROM release_grouping_member
          WHERE member_folder IN (OLD.folder, NEW.folder);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skip_added AFTER INSERT ON skipped_import_candidates
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder = NEW.candidate_path;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skip_removed AFTER DELETE ON skipped_import_candidates
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder = OLD.candidate_path;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skip_changed AFTER UPDATE ON skipped_import_candidates
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE folder IN (OLD.candidate_path, NEW.candidate_path);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skipped_tags_added AFTER INSERT ON scan_skipped_tags
BEGIN
    INSERT INTO import_list_replace (key) VALUES (NEW.candidate_path);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skipped_tags_removed AFTER DELETE ON scan_skipped_tags
BEGIN
    INSERT INTO import_list_replace (key) VALUES (OLD.candidate_path);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_skipped_tags_changed AFTER UPDATE ON scan_skipped_tags
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT OLD.candidate_path UNION SELECT NEW.candidate_path;
END;
