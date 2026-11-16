-- These views build the blank draft of every folder this device did not
-- import from what its scan stored: one empty track per audio unit
-- (`scan_candidate_unit`), each on the medium of its disc, with the files that
-- state no disc first. Every medium has no stated carrier and one side, and
-- every track takes the number its file states. A grouping's release takes
-- the grouping's name as its title, and any other release's title is empty.
-- The cover is the image file that the folder's cover rules pick
-- (`scan_candidate.blank_cover_file`).
--
-- These views are the one definition of that draft. The scan stores a new
-- state's draft from them when the folder's files hold no tags to draft from,
-- and so does the trigger that runs when bae deletes this device's release of
-- the folder (`local_release_origin_gone_seeds_blank_draft`). A state the scan
-- holds no audio units of yet (a folder that the scan found before it read
-- the folder's audio) has no rows in them.

-- This view joins each state to the first scan row of its candidate key and
-- content hash. The view leaves out a state the scan holds no audio units of
-- and a state whose release this device imported.
CREATE VIEW IF NOT EXISTS blank_draft_candidate AS
SELECT s.content_hash, scanned.watched_folder_path, scanned.path AS candidate_path,
       CASE WHEN scanned.grouping_key IS NULL THEN '' ELSE scanned.name END AS album_title,
       scanned.blank_cover_file
FROM import_candidate_state AS s
JOIN scan_candidate AS scanned
    ON scanned.path = s.candidate_key AND scanned.content_hash = s.content_hash
WHERE scanned.rowid = (
        SELECT MIN(copy.rowid) FROM scan_candidate AS copy
        WHERE copy.path = s.candidate_key AND copy.content_hash = s.content_hash)
  AND EXISTS (SELECT 1 FROM scan_candidate_unit AS unit
              WHERE unit.watched_folder_path = scanned.watched_folder_path
                AND unit.candidate_path = scanned.path)
  AND NOT EXISTS (SELECT 1 FROM local_release_origin AS origin
                  WHERE origin.content_hash = s.content_hash);

-- The draft row, in the columns of `import_candidate_edit`.
CREATE VIEW IF NOT EXISTS blank_draft_edit AS
SELECT content_hash, album_title, '' AS album_year, '' AS year, '[]' AS media,
       '[]' AS discogs_details, 'nobody' AS author,
       album_title = '' AS draft_blank, 0 AS draft_valid
FROM blank_draft_candidate;

-- The draft's tracks, in the columns of `import_candidate_track`. A track's
-- medium counts the discs below its own among the unit's candidate, plus one
-- for the files stating none.
CREATE VIEW IF NOT EXISTS blank_draft_track AS
SELECT candidate.content_hash, 'candidate-track-' || unit.position AS track_id,
       unit.position, '' AS title, 'album_artists' AS artist_assignment_kind,
       1 + (SELECT COUNT(DISTINCT other.disc) FROM scan_candidate_unit AS other
            WHERE other.watched_folder_path = unit.watched_folder_path
              AND other.candidate_path = unit.candidate_path AND other.disc < unit.disc)
         + (unit.disc IS NOT NULL
            AND EXISTS (SELECT 1 FROM scan_candidate_unit AS other
                        WHERE other.watched_folder_path = unit.watched_folder_path
                          AND other.candidate_path = unit.candidate_path
                          AND other.disc IS NULL)) AS medium,
       1 AS side, unit.number AS track_number, NULL AS stated_length_ms,
       unit.file_kind, unit.file_id, unit.sheet_id, unit.slice_index
FROM blank_draft_candidate AS candidate
JOIN scan_candidate_unit AS unit
    ON unit.watched_folder_path = candidate.watched_folder_path
   AND unit.candidate_path = candidate.candidate_path;

-- The draft's mediums, in the columns of `import_candidate_medium`.
CREATE VIEW IF NOT EXISTS blank_draft_medium AS
SELECT DISTINCT content_hash, medium AS position, NULL AS medium, 1 AS side_count
FROM blank_draft_track;

-- The draft's cover, in the columns of `import_candidate_cover`.
CREATE VIEW IF NOT EXISTS blank_draft_cover AS
SELECT content_hash, 'local' AS kind, blank_cover_file AS file_id
FROM blank_draft_candidate
WHERE blank_cover_file IS NOT NULL;

-- Deleting a folder's `local_release_origin` row writes the folder's blank
-- draft from its scanned files, so the folder is back in Found. The import
-- deleted the folder's draft, and no other bae write runs when the release
-- goes, so this trigger writes it. A state the scan holds no audio units for
-- has no blank draft in the views above: the scan writes that state's draft
-- when it reads the folder's files.
CREATE TRIGGER IF NOT EXISTS local_release_origin_gone_seeds_blank_draft
AFTER DELETE ON local_release_origin
WHEN NOT EXISTS (SELECT 1 FROM import_candidate_edit WHERE content_hash = OLD.content_hash)
BEGIN
    INSERT INTO import_candidate_edit
        (content_hash, album_title, album_year, year, media, discogs_details, author,
         draft_blank, draft_valid)
    SELECT content_hash, album_title, album_year, year, media, discogs_details, author,
           draft_blank, draft_valid
    FROM blank_draft_edit WHERE content_hash = OLD.content_hash;
    INSERT INTO import_candidate_medium (content_hash, position, medium, side_count)
    SELECT content_hash, position, medium, side_count
    FROM blank_draft_medium WHERE content_hash = OLD.content_hash;
    INSERT INTO import_candidate_track
        (content_hash, track_id, position, title, artist_assignment_kind, medium, side,
         track_number, stated_length_ms, file_kind, file_id, sheet_id, slice_index)
    SELECT content_hash, track_id, position, title, artist_assignment_kind, medium, side,
           track_number, stated_length_ms, file_kind, file_id, sheet_id, slice_index
    FROM blank_draft_track WHERE content_hash = OLD.content_hash;
    INSERT INTO import_candidate_cover (content_hash, kind, file_id)
    SELECT content_hash, kind, file_id
    FROM blank_draft_cover WHERE content_hash = OLD.content_hash;
END;
