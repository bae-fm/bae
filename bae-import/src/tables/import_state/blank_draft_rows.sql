-- The blank draft of every folder this device did not import, read from what
-- its scan stored: one empty track per audio unit (`scan_candidate_unit`),
-- each on the medium of its disc, files stating no disc first, every medium
-- of unstated carrier and one side, every track numbered as its file states.
-- A release a grouping builds is titled by the name it is listed as, and no
-- one folder's name speaks for any other release. The cover is the image of
-- its own the folder's rules pick (`scan_candidate.blank_cover_file`).
--
-- These views are the one definition of that draft. The scan seeds a state
-- from them, and so does the write that deletes this device's release of the
-- folder (`local_release_origin_gone_seeds_blank_draft`), which is why every
-- state of a folder this device did not import holds a draft row. A state the
-- scan holds no audio units of yet (a folder reported before its audio is
-- read) has no rows in them.

-- Each state, with the one scan copy of its files it names.
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

-- A folder whose release this device deleted (or another device deleted, and
-- coven applied here) is back in Found with the blank draft its files make:
-- the import deleted its draft, and no bae write runs when the release goes,
-- so the deletion seeds it. Every state of a folder this device did not
-- import then holds a draft row. A state the scan holds no audio units for
-- has no blank draft (the views above): the scan seeds it with its
-- files.
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
