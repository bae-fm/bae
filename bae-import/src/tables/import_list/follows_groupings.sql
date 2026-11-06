-- The placement follows the releases read from several folders picked
-- together: each change to a grouping or the folders it takes in places again
-- the releases it can move (`import_list_replace`). A change to a column the
-- rules don't read places nothing.

-- Removing a grouping removes its release, by foreign key, which places it
-- out of the list.
CREATE TRIGGER IF NOT EXISTS import_list_on_grouping_added AFTER INSERT ON release_grouping
BEGIN
    INSERT INTO import_list_replace (key) VALUES (NEW.key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_grouping_changed
AFTER UPDATE OF skipped, blocked ON release_grouping
WHEN (OLD.skipped, OLD.blocked) IS NOT (NEW.skipped, NEW.blocked)
BEGIN
    INSERT INTO import_list_replace (key) VALUES (NEW.key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_grouping_member_added
AFTER INSERT ON release_grouping_member
BEGIN
    INSERT INTO import_list_replace (key) VALUES (NEW.member_key), (NEW.grouping_key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_grouping_member_removed
AFTER DELETE ON release_grouping_member
BEGIN
    INSERT INTO import_list_replace (key) VALUES (OLD.member_key), (OLD.grouping_key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_grouping_member_changed
AFTER UPDATE ON release_grouping_member
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT OLD.member_key UNION SELECT OLD.grouping_key
    UNION SELECT NEW.member_key UNION SELECT NEW.grouping_key;
END;
