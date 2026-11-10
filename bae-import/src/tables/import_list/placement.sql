-- Where every row of the import list sits: one row per release the queue
-- lists, and one per header of a group on Found. Its tab, Found's state, the
-- group and dates it sorts by, the text it shows and what its commands are
-- decided from are worked out from a dozen tables, which no index reaches, so
-- triggers write them here, indexed, in the same write that changes any of
-- those tables. Computed from device-local rows and never synced.
CREATE TABLE IF NOT EXISTS import_list_placement (
    -- The row's number, which the list's search index (`import_list_search`)
    -- finds it by.
    entry               INTEGER PRIMARY KEY,
    watched_folder_path BLOB NOT NULL,
    -- 'candidate': a release; 'invalid': a folder that failed validation;
    -- 'group': the header of a group of Found's releases.
    item                TEXT NOT NULL CHECK (item IN ('candidate', 'invalid', 'group')),
    -- A release's key; a group's folder, the first name of its members'
    -- folders, as `RelativeFolder` bytes.
    key                 BLOB NOT NULL,
    -- The watched folder's position among the watched folders.
    root_position       INTEGER NOT NULL,
    tab                 TEXT NOT NULL CHECK (tab IN ('pending', 'done', 'skipped')),
    -- Where a Found release stands, what is running for it included.
    pending_state       TEXT CHECK (pending_state IN (
        'not_looked_up', 'identifying', 'needs_you', 'identified', 'unmatched',
        'lookup_error', 'error', 'importing', 'import_error')),
    -- The entry of Found's filter that holds a Found release.
    pending_entry       TEXT GENERATED ALWAYS AS (CASE
        WHEN pending_state IN ('needs_you', 'lookup_error', 'error', 'import_error')
            THEN 'needs_you'
        WHEN pending_state IN ('identifying', 'importing') THEN 'in_progress'
        ELSE pending_state END) VIRTUAL,
    -- Why a release that needs the person does.
    needs_you_reason    TEXT CHECK (needs_you_reason IN (
        'matches', 'no_tracklist', 'not_found', 'nothing_to_look_up')),
    -- How many pressings its lookup found, for a release that needs the
    -- person because it can't tell them apart.
    reason_pressings    INTEGER CHECK (reason_pressings >= 0),
    -- How bae broke on its own side, for a release in error.
    error_detail        TEXT,
    -- For a release at a watched folder's top level: whether another release
    -- is listed below its folder, which puts it under that folder's group.
    holds_nested        INTEGER NOT NULL DEFAULT 0 CHECK (holds_nested IN (0, 1)),
    -- A release's folder below its watched folder, as `RelativeFolder`
    -- bytes: each name's bytes, joined by `/`. NULL for a header.
    folder              BLOB,
    -- The text that shows it.
    display_path        TEXT GENERATED ALWAYS AS (coven_path_text(folder)) STORED,
    -- The group a Found release is listed under: the first name of its
    -- folder when it is nested, or its own folder when another release is
    -- listed below it. A header's own. Cut from the bytes (`instr` and
    -- `substr` count bytes on a BLOB), so a name that isn't valid Unicode
    -- groups as itself.
    group_folder        BLOB GENERATED ALWAYS AS (CASE
        WHEN item = 'group' THEN key
        WHEN item = 'candidate' AND tab = 'pending' THEN CASE
            WHEN instr(folder, X'2F') > 0
                THEN substr(folder, 1, instr(folder, X'2F') - 1)
            WHEN holds_nested THEN folder END
        END) STORED,
    -- The text that shows the group.
    group_name          TEXT GENERATED ALWAYS AS (coven_path_text(group_folder)) VIRTUAL,
    -- The release's own date: for Done the time its release was written, in
    -- Unix epoch milliseconds; otherwise its folder's date, as
    -- `folder_discovery` holds it. NULL when none is stored, and for a header.
    own_date            INTEGER,
    -- A group's date, its newest member's own date, on its header; NULL off
    -- a header. A member sorts in its header's place rather than by a copy
    -- of it, so a member that moves its group's date writes only the header.
    group_date          INTEGER,
    -- For a header: whether its folder is offered to be read as one release
    -- (`import_list_recombine`). NULL off a header.
    combinable          INTEGER CHECK (combinable IN (0, 1)),
    -- What a header or a row outside every group sorts by first: the
    -- group's date on a header, the row's own otherwise. NULL on a member,
    -- which sorts in its header's place.
    unit_date           INTEGER GENERATED ALWAYS AS (CASE
        WHEN item = 'group' THEN group_date
        WHEN group_folder IS NULL THEN own_date END) STORED,
    -- What the row sorts by after its date: its group's name, else its
    -- display path, so the list's order follows the text it shows.
    unit_path           TEXT GENERATED ALWAYS AS (COALESCE(group_name, display_path))
        VIRTUAL COLLATE natural_order,
    -- Whether a release shows a title and artists: always once this device
    -- imported it, and before then once its draft is not blank, or once a
    -- source or a link answered it. Otherwise it shows its folder's name.
    has_summary         INTEGER CHECK (has_summary IN (0, 1)),
    -- The text the row shows: an imported release's library album title and
    -- credits; else the draft's album title and album artists once it shows
    -- its draft, else the folder's name; for a folder that failed
    -- validation, what its tags call it.
    shown_title         TEXT,
    shown_artists       TEXT,
    -- The shown text case folded as `bae_fold_case` folds it, which is how
    -- the list's filter folds what is typed into it.
    filter_title        TEXT GENERATED ALWAYS AS (bae_fold_case(shown_title)) STORED,
    filter_artists      TEXT GENERATED ALWAYS AS (bae_fold_case(shown_artists)) STORED,
    -- What a release's commands are decided from, beside what is running for
    -- it: whether it can be acted on at all, whether its last import failed,
    -- whether its draft would import, what its stored lookup came to
    -- ('answered', 'failed', or NULL with none stored), and whether a
    -- grouping reads it from several folders. NULL off a release.
    actionable          INTEGER CHECK (actionable IN (0, 1)),
    import_failed       INTEGER CHECK (import_failed IN (0, 1)),
    draft_valid         INTEGER CHECK (draft_valid IN (0, 1)),
    lookup              TEXT CHECK (lookup IN ('answered', 'failed')),
    separable           INTEGER CHECK (separable IN (0, 1)),
    UNIQUE (watched_folder_path, item, key),
    CHECK (item <> 'invalid' OR tab = 'skipped'),
    CHECK ((tab = 'pending' AND item <> 'group') = (pending_state IS NOT NULL)),
    CHECK ((pending_state IS 'needs_you') = (needs_you_reason IS NOT NULL)),
    CHECK ((needs_you_reason IS 'matches') = (reason_pressings IS NOT NULL)),
    CHECK ((pending_state IS 'error') = (error_detail IS NOT NULL)),
    CHECK (holds_nested = 0 OR (item <> 'group' AND instr(folder, X'2F') = 0)),
    CHECK (item <> 'group' OR tab = 'pending'),
    CHECK (item = 'group' OR combinable IS NULL),
    CHECK ((item = 'group') = (folder IS NULL)),
    CHECK ((item = 'group') = (shown_title IS NULL)),
    CHECK ((item = 'candidate') = (has_summary IS NOT NULL)),
    CHECK ((item = 'candidate') = (actionable IS NOT NULL)),
    CHECK ((item = 'candidate') = (import_failed IS NOT NULL)),
    CHECK ((item = 'candidate') = (draft_valid IS NOT NULL)),
    CHECK ((item = 'candidate') = (separable IS NOT NULL)),
    CHECK (item = 'candidate' OR lookup IS NULL),
    -- A Done release always has the time its release was written; one stored
    -- in a form this can't read fails the write.
    CHECK (tab <> 'done' OR own_date IS NOT NULL)
) STRICT;

-- A release by its key alone, as a standing or a selection names it.
CREATE INDEX IF NOT EXISTS import_list_placement_by_key ON import_list_placement (key);

-- The release at a folder, and those nested below it.
CREATE INDEX IF NOT EXISTS import_list_placement_by_folder
    ON import_list_placement (watched_folder_path, folder);

-- Done's and Skipped's rows, which hold no group, in each order the list
-- shows them in; undated rows come last either way. Found's rows are read in
-- runs instead (below), so these hold none of them.
CREATE INDEX IF NOT EXISTS import_list_newest_first ON import_list_placement (
    tab, unit_date IS NULL, unit_date DESC, root_position, watched_folder_path, unit_path,
    item = 'group' DESC, own_date IS NULL, own_date DESC,
    display_path COLLATE natural_order, key)
    WHERE tab <> 'pending';

CREATE INDEX IF NOT EXISTS import_list_oldest_first ON import_list_placement (
    tab, unit_date IS NULL, unit_date, root_position, watched_folder_path, unit_path,
    item = 'group' DESC, own_date IS NULL, own_date, display_path COLLATE natural_order, key)
    WHERE tab <> 'pending';

CREATE INDEX IF NOT EXISTS import_list_path_ascending ON import_list_placement (
    tab, root_position, watched_folder_path, unit_path, item = 'group' DESC,
    display_path COLLATE natural_order, key)
    WHERE tab <> 'pending';

CREATE INDEX IF NOT EXISTS import_list_path_descending ON import_list_placement (
    tab, root_position DESC, watched_folder_path DESC, unit_path DESC, item = 'group' DESC,
    display_path COLLATE natural_order DESC, key DESC)
    WHERE tab <> 'pending';

-- Found reads each order in three runs, so a member sorts in its header's
-- place and no read passes a folded group's members: the headers and the
-- rows outside every group (`_unfolded`), each open group's header
-- (`_headers`), and that group's members in its order (`_members`), its
-- header leading them. Within the order's terms up to the folder, only a
-- header and its members tie, so the row's number breaks the tie in the
-- first two and the members take their header's terms and number: the three
-- runs then merge in the order's own sequence.
CREATE INDEX IF NOT EXISTS import_list_newest_first_unfolded ON import_list_placement (
    tab, unit_date IS NULL, unit_date DESC, root_position, watched_folder_path, unit_path, entry)
    WHERE item = 'group' OR group_folder IS NULL;

CREATE INDEX IF NOT EXISTS import_list_newest_first_headers ON import_list_placement (
    tab, unit_date IS NULL, unit_date DESC, root_position, watched_folder_path, unit_path, entry)
    WHERE item = 'group';

CREATE INDEX IF NOT EXISTS import_list_newest_first_members ON import_list_placement (
    watched_folder_path, group_folder, item = 'group' DESC, own_date IS NULL, own_date DESC,
    display_path COLLATE natural_order, key)
    WHERE item <> 'group' AND group_folder IS NOT NULL;

CREATE INDEX IF NOT EXISTS import_list_oldest_first_unfolded ON import_list_placement (
    tab, unit_date IS NULL, unit_date, root_position, watched_folder_path, unit_path, entry)
    WHERE item = 'group' OR group_folder IS NULL;

CREATE INDEX IF NOT EXISTS import_list_oldest_first_headers ON import_list_placement (
    tab, unit_date IS NULL, unit_date, root_position, watched_folder_path, unit_path, entry)
    WHERE item = 'group';

CREATE INDEX IF NOT EXISTS import_list_oldest_first_members ON import_list_placement (
    watched_folder_path, group_folder, item = 'group' DESC, own_date IS NULL, own_date,
    display_path COLLATE natural_order, key)
    WHERE item <> 'group' AND group_folder IS NOT NULL;

CREATE INDEX IF NOT EXISTS import_list_path_ascending_unfolded ON import_list_placement (
    tab, root_position, watched_folder_path, unit_path, entry)
    WHERE item = 'group' OR group_folder IS NULL;

CREATE INDEX IF NOT EXISTS import_list_path_ascending_headers ON import_list_placement (
    tab, root_position, watched_folder_path, unit_path, entry)
    WHERE item = 'group';

CREATE INDEX IF NOT EXISTS import_list_path_ascending_members ON import_list_placement (
    watched_folder_path, group_folder, item = 'group' DESC, display_path COLLATE natural_order, key)
    WHERE item <> 'group' AND group_folder IS NOT NULL;

CREATE INDEX IF NOT EXISTS import_list_path_descending_unfolded ON import_list_placement (
    tab, root_position DESC, watched_folder_path DESC, unit_path DESC, entry)
    WHERE item = 'group' OR group_folder IS NULL;

CREATE INDEX IF NOT EXISTS import_list_path_descending_headers ON import_list_placement (
    tab, root_position DESC, watched_folder_path DESC, unit_path DESC, entry)
    WHERE item = 'group';

CREATE INDEX IF NOT EXISTS import_list_path_descending_members ON import_list_placement (
    watched_folder_path, group_folder, item = 'group' DESC,
    display_path COLLATE natural_order DESC, key DESC)
    WHERE item <> 'group' AND group_folder IS NOT NULL;

-- The releases the queue lists, each as the row it is listed as: settled
-- (one found before the folder around it was known is not listed yet), not
-- taken in by a grouping, and under a watched folder.
CREATE VIEW IF NOT EXISTS import_list_listed AS
SELECT c.watched_folder_path, c.path AS key,
       CASE c.kind WHEN 'invalid' THEN 'invalid' ELSE 'candidate' END AS item,
       w.position AS root_position
FROM scan_candidate AS c
JOIN watched_import_folders AS w ON w.path = c.watched_folder_path
WHERE c.kind <> 'tentative'
  AND NOT EXISTS (SELECT 1 FROM release_grouping_member AS member
                  WHERE member.member_key = c.path);

-- Every release the queue lists, placed: the rules the list's tabs, Found's
-- states, dates, shown text and commands follow, read one release at a time
-- by the triggers that keep `import_list_placement`. Groups are the
-- placement's own (`group_folder`).
CREATE VIEW IF NOT EXISTS import_list_entry AS
SELECT watched_folder_path, item, key, root_position, tab, pending_state, needs_you_reason,
       CASE WHEN needs_you_reason = 'matches' THEN pressings END AS reason_pressings,
       -- A run's answer bae could not store says how it broke; otherwise the
       -- stored verdict does.
       CASE WHEN pending_state = 'error' THEN
                CASE WHEN identification IN ('not_written', 'inapplicable')
                     THEN save_failure_error ELSE verdict_error END
            END AS error_detail,
       own_date, folder, has_summary, shown_title, shown_artists,
       actionable, import_failed, draft_valid, lookup, separable
FROM (
    SELECT judged.*,
           -- What is running for a Found release decides its state over the
           -- tables: an import, then a run, then a run's answer bae could
           -- not store.
           CASE WHEN tab <> 'pending' THEN NULL
                WHEN running_import IS NOT NULL THEN 'importing'
                WHEN identification IN ('queued', 'running', 'finalizing') THEN 'identifying'
                WHEN identification = 'reading_pick' THEN 'identified'
                WHEN identification IN ('not_written', 'inapplicable') THEN 'error'
                ELSE stored_state END AS pending_state,
           CASE WHEN tab = 'pending' AND running_import IS NULL AND identification IS NULL
                THEN stored_reason END AS needs_you_reason,
           CASE WHEN tab = 'done' THEN imported_at ELSE discovered_at END AS own_date,
           -- An imported release shows its library album's title and
           -- credits, as the library holds them now.
           CASE WHEN item = 'invalid' THEN COALESCE(tag_title, name)
                WHEN imported THEN library_title
                WHEN has_summary THEN COALESCE(draft_title, name)
                ELSE name END AS shown_title,
           CASE WHEN item = 'invalid' THEN tag_artist
                WHEN imported THEN COALESCE(library_credit_line, '')
                WHEN has_summary THEN COALESCE(credit_line, '') END AS shown_artists,
           CASE WHEN item = 'candidate' THEN tab = 'pending' AND failed END AS import_failed,
           CASE WHEN item = 'candidate' THEN
                -- What the stored lookup came to: failed when a source could
                -- not answer, bae broke, or a found release could not be read
                -- in full.
                CASE WHEN verdict_kind IS NULL THEN NULL
                     WHEN verdict_kind IN ('failed', 'error') THEN 'failed'
                     WHEN declined IS 'unread_document' THEN 'failed'
                     ELSE 'answered' END
           END AS lookup
    FROM (
        SELECT facts.*,
               CASE WHEN kind = 'invalid' THEN 'skipped'
                    WHEN imported THEN 'done'
                    WHEN skipped THEN 'skipped'
                    ELSE 'pending' END AS tab,
               -- Where the tables put a Found release.
               CASE WHEN failed THEN 'import_error'
                    WHEN linked THEN 'identified'
                    WHEN verdict_kind IS NULL THEN 'not_looked_up'
                    WHEN kept_own_draft THEN 'unmatched'
                    WHEN verdict_kind = 'error' THEN 'error'
                    WHEN verdict_kind = 'failed' THEN 'lookup_error'
                    WHEN verdict_kind IN ('not_found', 'manual_only') THEN 'needs_you'
                    WHEN declined IS NULL THEN 'unmatched'
                    WHEN declined = 'unread_document' THEN 'lookup_error'
                    ELSE 'needs_you' END AS stored_state,
               CASE WHEN failed OR linked OR verdict_kind IS NULL OR kept_own_draft THEN NULL
                    WHEN verdict_kind = 'not_found' THEN 'not_found'
                    WHEN verdict_kind = 'manual_only' THEN 'nothing_to_look_up'
                    WHEN verdict_kind <> 'found' THEN NULL
                    WHEN declined = 'nothing_found' THEN 'not_found'
                    WHEN declined = 'several' THEN 'matches'
                    WHEN declined = 'source_tracks_unknown' THEN 'no_tracklist'
                    WHEN declined = 'medium_disagrees' THEN 'not_found' END AS stored_reason,
               -- The time the release was written, read off its RFC 3339
               -- text: whole seconds through its offset, and the
               -- milliseconds of its fraction, truncated.
               unixepoch(substr(imported_at_text, 1, 19) || imported_at_offset) * 1000
                   + CAST(substr(imported_at_fraction || '000', 1, 3) AS INTEGER) AS imported_at
        FROM (
            SELECT listed.watched_folder_path, listed.key, listed.item, listed.root_position,
                   c.kind, c.relative_folder AS folder, c.name,
                   -- A scanned release is dated by its folder; a grouping's
                   -- by the earliest of the folders it takes in.
                   CASE c.source_kind
                       WHEN 'folder' THEN
                           (SELECT COALESCE(d.source_date, d.first_seen_at)
                            FROM folder_discovery AS d WHERE d.folder = c.folder)
                       WHEN 'grouping' THEN
                           (SELECT MIN(COALESCE(d.source_date, d.first_seen_at))
                            FROM release_grouping_member AS m
                            JOIN folder_discovery AS d ON d.folder = m.member_folder
                            WHERE m.grouping_key = c.grouping_key)
                   END AS discovered_at,
                   -- Only what this device imported places a folder in Done.
                   origin.release_id IS NOT NULL AS imported,
                   released.created_at AS imported_at_text,
                   CASE WHEN substr(released.created_at, 20, 1) = '.'
                        THEN ltrim(substr(released.created_at, 21), '0123456789')
                        ELSE substr(released.created_at, 20) END AS imported_at_offset,
                   CASE WHEN substr(released.created_at, 20, 1) = '.'
                        THEN substr(released.created_at, 21,
                                    length(released.created_at) - 20
                                        - length(ltrim(substr(released.created_at, 21),
                                                       '0123456789')))
                        ELSE '' END AS imported_at_fraction,
                   CASE WHEN c.grouping_key IS NOT NULL THEN g.skipped
                        ELSE EXISTS (SELECT 1 FROM skipped_import_candidates AS k
                                     WHERE k.candidate_path = c.folder) END AS skipped,
                   g.blocked IS NOT NULL
                       OR EXISTS (SELECT 1 FROM import_candidate_failure AS f
                                  WHERE f.content_hash = c.content_hash) AS failed,
                   CASE WHEN listed.item = 'candidate' THEN g.blocked IS NULL END AS actionable,
                   CASE WHEN listed.item = 'candidate' THEN c.grouping_key IS NOT NULL
                        END AS separable,
                   CASE WHEN listed.item = 'candidate' THEN COALESCE(e.draft_valid, 0)
                        END AS draft_valid,
                   -- The draft, the link and the verdict count only while the
                   -- state describes the folder's current files.
                   EXISTS (SELECT 1 FROM import_candidate_release_link AS l
                           WHERE l.content_hash = s.content_hash) AS linked,
                   v.kind AS verdict_kind,
                   v.kept_own_draft,
                   v.error_detail AS verdict_error,
                   -- How many pressings the releases agreement kept make.
                   (SELECT COUNT(DISTINCT m.pressing) FROM import_candidate_match AS m
                    WHERE m.content_hash = v.content_hash AND m.listing = 'offered') AS pressings,
                   -- Why a found verdict picks none of its releases
                   -- unattended, judged from what it found that agreement
                   -- kept, led by the lowest position.
                   CASE WHEN v.kind <> 'found' THEN NULL
                        WHEN v.medium_conflict IS NOT NULL THEN 'medium_disagrees'
                        ELSE (SELECT CASE
                                  WHEN lead.position IS NULL THEN 'nothing_found'
                                  WHEN (SELECT COUNT(DISTINCT m.pressing)
                                        FROM import_candidate_match AS m
                                        WHERE m.content_hash = v.content_hash
                                          AND m.listing = 'offered') <> 1
                                      THEN 'several'
                                  WHEN EXISTS (SELECT 1 FROM import_candidate_match AS m
                                               WHERE m.content_hash = v.content_hash
                                                 AND m.listing = 'offered'
                                                 AND m.document_failure IS NOT NULL)
                                      THEN 'unread_document'
                                  WHEN lead.source_tracks_kind = 'listed'
                                   AND lead.source_tracks_count = COALESCE(v.track_count, 0)
                                      THEN NULL
                                  WHEN lead.source_tracks_kind IN ('listed', 'unreadable')
                                      THEN 'nothing_found'
                                  ELSE 'source_tracks_unknown' END
                              FROM (SELECT NULL) AS one
                              LEFT JOIN import_candidate_match AS lead
                                  ON lead.content_hash = v.content_hash
                                 AND lead.position = (SELECT MIN(m.position)
                                                      FROM import_candidate_match AS m
                                                      WHERE m.content_hash = v.content_hash
                                                        AND m.listing = 'offered'))
                   END AS declined,
                   st.identification, st.import_step AS running_import, st.save_failure_error,
                   t.album_title AS tag_title, t.artist AS tag_artist,
                   e.album_title AS draft_title,
                   album.title AS library_title,
                   -- The library album's credits as one line, as the library
                   -- prints them: each credit's printed name (its credit
                   -- name, else the name of the artist it shows as), after
                   -- the join the credit before it states, or a comma.
                   (SELECT group_concat(
                           COALESCE((SELECT COALESCE(before.join_phrase, ', ')
                                     FROM album_artists AS before
                                     WHERE before.album_id = credit.album_id
                                       AND before.position < credit.position
                                     ORDER BY before.position DESC LIMIT 1), '')
                           || COALESCE(printed.name, shown.name),
                           '' ORDER BY credit.position)
                    FROM album_artists AS credit
                    JOIN artist_shown_as AS shown_as ON shown_as.artist_id = credit.artist_id
                    JOIN artists AS shown ON shown.id = shown_as.shown_artist_id
                    LEFT JOIN artist_credits AS printed
                        ON printed.artist_id = credit.artist_id
                       AND printed.name_key = credit.credit_key
                    WHERE credit.album_id = album.id) AS library_credit_line,
                   CASE WHEN listed.item = 'candidate' THEN
                       origin.release_id IS NOT NULL
                       OR (e.content_hash IS NOT NULL
                           AND (NOT e.draft_blank
                                OR EXISTS (SELECT 1 FROM import_candidate_draft_provenance AS p
                                           WHERE p.content_hash = e.content_hash)
                                OR EXISTS (SELECT 1 FROM import_candidate_release_link AS l
                                           WHERE l.content_hash = e.content_hash)))
                       -- With no draft row the candidate holds its files'
                       -- blank draft, which titles a release read from
                       -- several folders by the name it is listed as.
                       OR (s.content_hash IS NOT NULL AND e.content_hash IS NULL
                           AND c.grouping_key IS NOT NULL)
                   END AS has_summary,
                   -- The draft's album credits as one line: each printed
                   -- name, after the join the credit before it states, or a
                   -- comma.
                   (SELECT group_concat(
                           COALESCE((SELECT COALESCE(before.join_phrase, ', ')
                                     FROM import_candidate_album_artist_assignment AS before
                                     WHERE before.content_hash = a.content_hash
                                       AND before.position < a.position
                                     ORDER BY before.position DESC LIMIT 1), '')
                           || CASE a.assignment_kind
                                  WHEN 'picked' THEN COALESCE(credit.name, artist.name)
                                  ELSE COALESCE(a.credited_as, a.name) END,
                           '' ORDER BY a.position)
                    FROM import_candidate_album_artist_assignment AS a
                    LEFT JOIN artists AS artist ON artist.id = a.artist_id
                    LEFT JOIN artist_credits AS credit
                        ON credit.artist_id = a.artist_id AND credit.name_key = a.credit_key
                    WHERE a.content_hash = e.content_hash) AS credit_line
            FROM import_list_listed AS listed
            JOIN scan_candidate AS c
                ON c.watched_folder_path = listed.watched_folder_path AND c.path = listed.key
            LEFT JOIN release_grouping AS g ON g.key = c.grouping_key
            LEFT JOIN import_candidate_state AS s
                ON s.content_hash = c.content_hash AND s.edit_revision = c.file_edit_revision
            LEFT JOIN import_candidate_edit AS e ON e.content_hash = s.content_hash
            LEFT JOIN import_candidate_verdict AS v ON v.content_hash = s.content_hash
            LEFT JOIN local_release_origin AS origin ON origin.content_hash = c.content_hash
            LEFT JOIN releases AS released ON released.id = origin.release_id
            LEFT JOIN albums AS album ON album.id = released.album_id
            LEFT JOIN import_candidate_standing AS st ON st.candidate_key = c.path
            LEFT JOIN scan_skipped_tags AS t
                ON t.watched_folder_path = c.watched_folder_path AND t.candidate_path = c.path
        ) AS facts
    ) AS judged
) AS placed;

-- Place the release at `key` again from its tables: SQLite has no procedures,
-- so the triggers that keep `import_list_placement` name a release to place
-- by inserting its key here, and the trigger on this view does the placing.
-- Nothing is ever stored in it.
CREATE VIEW IF NOT EXISTS import_list_replace AS
SELECT path AS key FROM scan_candidate WHERE 0;

CREATE TRIGGER IF NOT EXISTS import_list_replace_release INSTEAD OF INSERT ON import_list_replace
BEGIN
    INSERT INTO import_list_placement
        (watched_folder_path, item, key, root_position, tab, pending_state, needs_you_reason,
         reason_pressings, error_detail, own_date, folder, has_summary,
         shown_title, shown_artists, actionable, import_failed, draft_valid, lookup, separable)
    SELECT watched_folder_path, item, key, root_position, tab, pending_state, needs_you_reason,
           reason_pressings, error_detail, own_date, folder, has_summary,
           shown_title, shown_artists, actionable, import_failed, draft_valid, lookup, separable
    FROM import_list_entry WHERE key = NEW.key
    ON CONFLICT (watched_folder_path, item, key) DO UPDATE SET
        root_position = excluded.root_position,
        tab = excluded.tab,
        pending_state = excluded.pending_state,
        needs_you_reason = excluded.needs_you_reason,
        reason_pressings = excluded.reason_pressings,
        error_detail = excluded.error_detail,
        own_date = excluded.own_date,
        folder = excluded.folder,
        has_summary = excluded.has_summary,
        shown_title = excluded.shown_title,
        shown_artists = excluded.shown_artists,
        actionable = excluded.actionable,
        import_failed = excluded.import_failed,
        draft_valid = excluded.draft_valid,
        lookup = excluded.lookup,
        separable = excluded.separable
    WHERE (import_list_placement.root_position, import_list_placement.tab,
           import_list_placement.pending_state, import_list_placement.needs_you_reason,
           import_list_placement.reason_pressings,
           import_list_placement.error_detail, import_list_placement.own_date,
           import_list_placement.folder, import_list_placement.has_summary,
           import_list_placement.shown_title, import_list_placement.shown_artists,
           import_list_placement.actionable, import_list_placement.import_failed,
           import_list_placement.draft_valid, import_list_placement.lookup,
           import_list_placement.separable)
        IS NOT (excluded.root_position, excluded.tab, excluded.pending_state,
                excluded.needs_you_reason, excluded.reason_pressings,
                excluded.error_detail, excluded.own_date, excluded.folder,
                excluded.has_summary, excluded.shown_title, excluded.shown_artists,
                excluded.actionable, excluded.import_failed, excluded.draft_valid,
                excluded.lookup, excluded.separable);
    -- A release that is no longer this item, or no longer listed, goes;
    -- after its new row is in, so what it was selected as stays selected.
    DELETE FROM import_list_placement
    WHERE key = NEW.key AND item <> 'group'
      AND NOT EXISTS (SELECT 1 FROM import_list_listed AS listed
                      WHERE listed.key = NEW.key
                        AND listed.watched_folder_path = import_list_placement.watched_folder_path
                        AND listed.item = import_list_placement.item);
END;
