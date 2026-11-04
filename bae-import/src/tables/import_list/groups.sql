-- Bring the header of the group `group_folder` under `watched_folder_path` in
-- line with its members: there while it has any, and dated by its newest
-- member's own date. Like `import_list_replace`, a procedure entry
-- point that stores nothing.
CREATE VIEW IF NOT EXISTS import_list_regroup AS
SELECT watched_folder_path, group_folder FROM import_list_placement WHERE 0;

CREATE TRIGGER IF NOT EXISTS import_list_regroup_header INSTEAD OF INSERT ON import_list_regroup
BEGIN
    INSERT INTO import_list_placement
        (watched_folder_path, item, key, root_position, tab)
    SELECT NEW.watched_folder_path, 'group', NEW.group_folder, w.position, 'pending'
    FROM watched_import_folders AS w
    WHERE w.path = NEW.watched_folder_path
      AND EXISTS (SELECT 1 FROM import_list_placement AS member
                  WHERE member.watched_folder_path = NEW.watched_folder_path
                    AND member.group_folder = NEW.group_folder AND member.item <> 'group')
    ON CONFLICT (watched_folder_path, item, key) DO NOTHING;
    -- A header just placed is told whether its folder reads as one release.
    INSERT INTO import_list_recombine (watched_folder_path, group_folder)
    SELECT NEW.watched_folder_path, NEW.group_folder
    WHERE EXISTS (SELECT 1 FROM import_list_placement AS header
                  WHERE header.watched_folder_path = NEW.watched_folder_path
                    AND header.item = 'group' AND header.key = NEW.group_folder
                    AND header.combinable IS NULL);
    DELETE FROM import_list_placement
    WHERE watched_folder_path = NEW.watched_folder_path AND item = 'group' AND key = NEW.group_folder
      AND NOT EXISTS (SELECT 1 FROM import_list_placement AS member
                      WHERE member.watched_folder_path = NEW.watched_folder_path
                        AND member.group_folder = NEW.group_folder AND member.item <> 'group');
    -- The newest member is the first of the group's members newest first,
    -- read off `import_list_newest_first_members` without passing the rest.
    UPDATE import_list_placement
    SET group_date = newest.own_date
    FROM (SELECT member.own_date FROM import_list_placement AS member
          WHERE member.watched_folder_path = NEW.watched_folder_path
            AND member.group_folder = NEW.group_folder AND member.item <> 'group'
          ORDER BY member.item = 'group' DESC, member.own_date IS NULL, member.own_date DESC
          LIMIT 1) AS newest
    WHERE watched_folder_path = NEW.watched_folder_path AND item = 'group' AND key = NEW.group_folder
      AND group_date IS NOT newest.own_date;
END;

-- Bring whether the folder of the group `group_folder` under
-- `watched_folder_path` is offered to be read as one release in line with
-- the rows below it and the readings stored for it and the folders inside
-- it, on its header. It is when a stored reading of the folder keeps its
-- releases apart, or, with nothing stored, when it is the nearest folder
-- above some release holding two releases or more: no folder between the
-- two has nothing stored and two releases below it. Rows of every tab count.
-- A procedure entry point that stores nothing, like `import_list_replace`.
--
-- The folders below one are the range from its bytes and `/` up to its bytes
-- and the byte after `/`. `||` gives TEXT, and every TEXT sorts below every
-- BLOB, so each bound is cast back to the BLOB its bytes make.
CREATE VIEW IF NOT EXISTS import_list_recombine AS
SELECT watched_folder_path, group_folder FROM import_list_placement WHERE 0;

CREATE TRIGGER IF NOT EXISTS import_list_recombine_header
INSTEAD OF INSERT ON import_list_recombine
WHEN EXISTS (SELECT 1 FROM import_list_placement AS header
             WHERE header.watched_folder_path = NEW.watched_folder_path
               AND header.item = 'group' AND header.key = NEW.group_folder)
BEGIN
    UPDATE import_list_placement SET combinable = judged.combinable
    FROM (SELECT COALESCE(
              (SELECT g.combined = 0 FROM release_grouping AS g
               WHERE g.anchor_folder = bae_folder_below(NEW.watched_folder_path, NEW.group_folder)),
              (SELECT count(*) FROM (
                   SELECT 1 FROM import_list_placement AS q
                   WHERE q.watched_folder_path = NEW.watched_folder_path
                     AND q.folder > CAST(NEW.group_folder || X'2F' AS BLOB)
                     AND q.folder < CAST(NEW.group_folder || X'30' AS BLOB) LIMIT 2)) = 2
              AND EXISTS (
                  SELECT 1 FROM import_list_placement AS b
                  WHERE b.watched_folder_path = NEW.watched_folder_path
                    AND b.folder > CAST(NEW.group_folder || X'2F' AS BLOB)
                    AND b.folder < CAST(NEW.group_folder || X'30' AS BLOB)
                    AND NOT EXISTS (
                        WITH RECURSIVE above (folder, rest) AS (
                            SELECT NEW.group_folder,
                                   substr(b.folder, length(NEW.group_folder) + 2)
                            UNION ALL
                            SELECT CAST(folder || X'2F' || substr(rest, 1, instr(rest, X'2F') - 1)
                                        AS BLOB),
                                   substr(rest, instr(rest, X'2F') + 1)
                            FROM above WHERE instr(rest, X'2F') > 0)
                        SELECT 1 FROM above AS f
                        WHERE f.folder <> NEW.group_folder
                          AND (SELECT count(*) FROM (
                                   SELECT 1 FROM import_list_placement AS q
                                   WHERE q.watched_folder_path = NEW.watched_folder_path
                                     AND q.folder > CAST(f.folder || X'2F' AS BLOB)
                                     AND q.folder < CAST(f.folder || X'30' AS BLOB) LIMIT 2)) = 2
                          AND NOT EXISTS (
                              SELECT 1 FROM release_grouping AS g
                              WHERE g.anchor_folder =
                                  bae_folder_below(NEW.watched_folder_path, f.folder)))))
              AS combinable) AS judged
    WHERE import_list_placement.watched_folder_path = NEW.watched_folder_path
      AND import_list_placement.item = 'group' AND import_list_placement.key = NEW.group_folder
      AND import_list_placement.combinable IS NOT judged.combinable;
END;

-- A stored reading of a folder, or of one inside it, changes whether the
-- group holding it is offered to be read as one release.
CREATE TRIGGER IF NOT EXISTS import_list_reading_added AFTER INSERT ON release_grouping
WHEN NEW.anchor_folder IS NOT NULL
BEGIN
    INSERT INTO import_list_recombine (watched_folder_path, group_folder)
    SELECT r.root, CASE WHEN instr(r.folder, X'2F') > 0
                        THEN substr(r.folder, 1, instr(r.folder, X'2F') - 1) ELSE r.folder END
    FROM (SELECT w.path AS root, bae_folder_relative(w.path, NEW.anchor_folder) AS folder
          FROM watched_import_folders AS w) AS r
    WHERE r.folder <> X'';
END;

CREATE TRIGGER IF NOT EXISTS import_list_reading_removed AFTER DELETE ON release_grouping
WHEN OLD.anchor_folder IS NOT NULL
BEGIN
    INSERT INTO import_list_recombine (watched_folder_path, group_folder)
    SELECT r.root, CASE WHEN instr(r.folder, X'2F') > 0
                        THEN substr(r.folder, 1, instr(r.folder, X'2F') - 1) ELSE r.folder END
    FROM (SELECT w.path AS root, bae_folder_relative(w.path, OLD.anchor_folder) AS folder
          FROM watched_import_folders AS w) AS r
    WHERE r.folder <> X'';
END;

CREATE TRIGGER IF NOT EXISTS import_list_reading_changed
AFTER UPDATE OF anchor_folder, combined ON release_grouping
WHEN (OLD.anchor_folder, OLD.combined) IS NOT (NEW.anchor_folder, NEW.combined)
BEGIN
    INSERT INTO import_list_recombine (watched_folder_path, group_folder)
    SELECT r.root, CASE WHEN instr(r.folder, X'2F') > 0
                        THEN substr(r.folder, 1, instr(r.folder, X'2F') - 1) ELSE r.folder END
    FROM (SELECT w.path AS root, bae_folder_relative(w.path, OLD.anchor_folder) AS folder
          FROM watched_import_folders AS w
          UNION
          SELECT w.path, bae_folder_relative(w.path, NEW.anchor_folder)
          FROM watched_import_folders AS w) AS r
    WHERE r.folder <> X'';
END;

-- Whether another release is listed below the folder of each row at a
-- watched folder's top level: what puts that row under its folder's group.
CREATE VIEW IF NOT EXISTS import_list_nesting AS
SELECT p.watched_folder_path, p.item, p.key, p.folder,
       EXISTS (SELECT 1 FROM import_list_placement AS nested
               WHERE nested.watched_folder_path = p.watched_folder_path
                 AND nested.folder > CAST(p.folder || X'2F' AS BLOB)
                 AND nested.folder < CAST(p.folder || X'30' AS BLOB)) AS holds_nested
FROM import_list_placement AS p
WHERE p.item <> 'group' AND instr(p.folder, X'2F') = 0;

-- Bring whether the top-level row at `folder` under `watched_folder_path`
-- holds a nested release in line with the rows below it. A procedure entry
-- point that stores nothing, like `import_list_replace`.
CREATE VIEW IF NOT EXISTS import_list_renest AS
SELECT watched_folder_path, folder FROM import_list_placement WHERE 0;

CREATE TRIGGER IF NOT EXISTS import_list_renest_row INSTEAD OF INSERT ON import_list_renest
BEGIN
    UPDATE import_list_placement
    SET holds_nested = nesting.holds_nested
    FROM import_list_nesting AS nesting
    WHERE nesting.watched_folder_path = NEW.watched_folder_path
      AND nesting.folder = NEW.folder
      AND import_list_placement.watched_folder_path = nesting.watched_folder_path
      AND import_list_placement.item = nesting.item
      AND import_list_placement.key = nesting.key
      AND import_list_placement.holds_nested IS NOT nesting.holds_nested;
END;

-- A row placed, moved or gone changes the group it is listed under, and
-- whether the top-level row at the folder it is in holds a nested release:
-- itself, when it is that row.
CREATE TRIGGER IF NOT EXISTS import_list_placement_added AFTER INSERT ON import_list_placement
WHEN NEW.item <> 'group'
BEGIN
    INSERT INTO import_list_renest (watched_folder_path, folder)
    VALUES (NEW.watched_folder_path,
            CASE WHEN instr(NEW.folder, X'2F') > 0
                 THEN substr(NEW.folder, 1, instr(NEW.folder, X'2F') - 1)
                 ELSE NEW.folder END);
    INSERT INTO import_list_regroup (watched_folder_path, group_folder)
    SELECT NEW.watched_folder_path, NEW.group_folder WHERE NEW.group_folder IS NOT NULL;
    INSERT INTO import_list_recombine (watched_folder_path, group_folder)
    SELECT NEW.watched_folder_path, substr(NEW.folder, 1, instr(NEW.folder, X'2F') - 1)
    WHERE instr(NEW.folder, X'2F') > 0;
END;

CREATE TRIGGER IF NOT EXISTS import_list_placement_removed AFTER DELETE ON import_list_placement
WHEN OLD.item <> 'group'
BEGIN
    INSERT INTO import_list_renest (watched_folder_path, folder)
    SELECT OLD.watched_folder_path,
           substr(OLD.folder, 1, instr(OLD.folder, X'2F') - 1)
    WHERE instr(OLD.folder, X'2F') > 0;
    INSERT INTO import_list_regroup (watched_folder_path, group_folder)
    SELECT OLD.watched_folder_path, OLD.group_folder WHERE OLD.group_folder IS NOT NULL;
    INSERT INTO import_list_recombine (watched_folder_path, group_folder)
    SELECT OLD.watched_folder_path, substr(OLD.folder, 1, instr(OLD.folder, X'2F') - 1)
    WHERE instr(OLD.folder, X'2F') > 0;
END;

CREATE TRIGGER IF NOT EXISTS import_list_placement_moved AFTER UPDATE ON import_list_placement
WHEN NEW.item <> 'group' AND OLD.folder IS NOT NEW.folder
BEGIN
    INSERT INTO import_list_renest (watched_folder_path, folder)
    SELECT OLD.watched_folder_path,
           substr(OLD.folder, 1, instr(OLD.folder, X'2F') - 1)
    WHERE instr(OLD.folder, X'2F') > 0
    UNION
    SELECT NEW.watched_folder_path,
           CASE WHEN instr(NEW.folder, X'2F') > 0
                THEN substr(NEW.folder, 1, instr(NEW.folder, X'2F') - 1)
                ELSE NEW.folder END;
    INSERT INTO import_list_recombine (watched_folder_path, group_folder)
    SELECT OLD.watched_folder_path, substr(OLD.folder, 1, instr(OLD.folder, X'2F') - 1)
    WHERE instr(OLD.folder, X'2F') > 0
    UNION
    SELECT NEW.watched_folder_path, substr(NEW.folder, 1, instr(NEW.folder, X'2F') - 1)
    WHERE instr(NEW.folder, X'2F') > 0;
END;

CREATE TRIGGER IF NOT EXISTS import_list_placement_regrouped AFTER UPDATE ON import_list_placement
WHEN NEW.item <> 'group'
 AND (OLD.group_folder IS NOT NEW.group_folder OR OLD.own_date IS NOT NEW.own_date)
BEGIN
    INSERT INTO import_list_regroup (watched_folder_path, group_folder)
    SELECT OLD.watched_folder_path, OLD.group_folder WHERE OLD.group_folder IS NOT NULL
    UNION
    SELECT NEW.watched_folder_path, NEW.group_folder WHERE NEW.group_folder IS NOT NULL;
END;
