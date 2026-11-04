-- Every path on disk in these tables is coven's bytes for it
-- (`coven::path_bytes`), so a folder whose name isn't valid Unicode keys its
-- own rows; `coven_path_text` shows one.

-- Folders read as one release, or a folder whose releases are kept apart.
-- A grouping anchored at a folder reads the releases below it as one or keeps
-- them apart; the scan proposes one where a folder yields several releases,
-- and the user's answer replaces it. A grouping with no anchor combines the
-- releases its members name, from anywhere, and keeps them out of the queue
-- while it stands; its release is listed under its first member's watched
-- folder. Folders are named by their paths on disk, so a folder taking over
-- the watched folders inside it keeps their groupings; removing the watched
-- folder covering one deletes it (`remove_watched_import_folders`).
CREATE TABLE IF NOT EXISTS release_grouping (
    -- The candidate key of the release the grouping reads as one.
    key                  BLOB PRIMARY KEY,
    -- The folder an anchored grouping reads; NULL for one with no anchor.
    anchor_folder        BLOB UNIQUE,
    combined             INTEGER NOT NULL CHECK (combined IN (0, 1)),
    -- 'heuristic' when the scan decided because nothing was stored; 'user' for
    -- the user's answer, which the scan never overrides.
    author               TEXT NOT NULL CHECK (author IN ('user', 'heuristic')),
    skipped              INTEGER NOT NULL DEFAULT 0 CHECK (skipped IN (0, 1)),
    -- Why a grouping with no anchor cannot be worked on: a release it takes in
    -- changed or is gone, its folder's files go with another release or are
    -- still downloading, or its releases make no release. The release keeps
    -- what it was last built from until this is fixed or the grouping is
    -- undone. `blocked_subject` names the folder (for 'unbuildable', the
    -- diagnostic) and `blocked_holder` the release already reading the
    -- folder's files; both are for the log.
    blocked              TEXT CHECK (blocked IS NULL OR blocked IN (
        'source_changed', 'source_gone', 'folder_files_taken',
        'folder_files_contested', 'folder_files_downloading', 'unbuildable'
    )),
    blocked_subject      TEXT,
    blocked_holder       TEXT,
    -- For a grouping with no anchor: the folder all its releases sit directly
    -- in, and whether its release reads that folder's sidecar files
    -- (scan_sidecar). At most one release reads them: while several groupings
    -- sit in a folder with files a new one is refused, and a rebuild blocks
    -- all but the reader.
    parent_folder        BLOB,
    reads_parent_files   INTEGER NOT NULL DEFAULT 0 CHECK (reads_parent_files IN (0, 1)),
    CHECK (anchor_folder IS NOT NULL OR (combined = 1 AND author = 'user')),
    CHECK (anchor_folder IS NULL OR blocked IS NULL),
    CHECK ((blocked IS NULL) = (blocked_subject IS NULL)),
    CHECK ((blocked IS 'folder_files_taken') = (blocked_holder IS NOT NULL)),
    CHECK (anchor_folder IS NULL OR parent_folder IS NULL),
    CHECK (reads_parent_files = 0 OR parent_folder IS NOT NULL)
) STRICT;

-- The groupings sitting in a folder, and the one that reads its files.
CREATE INDEX IF NOT EXISTS release_grouping_by_parent ON release_grouping (parent_folder);

CREATE UNIQUE INDEX IF NOT EXISTS release_grouping_parent_reader
    ON release_grouping (parent_folder) WHERE reads_parent_files = 1;

-- The releases a grouping with no anchor takes in, in play order. A grouping
-- goes once no watched folder covers a folder it takes a release from.
CREATE TABLE IF NOT EXISTS release_grouping_member (
    grouping_key  BLOB NOT NULL
        REFERENCES release_grouping (key) ON DELETE CASCADE,
    position      INTEGER NOT NULL CHECK (position >= 0),
    member_key    BLOB NOT NULL UNIQUE,
    -- The folder on disk the member release is read from.
    member_folder BLOB NOT NULL,
    PRIMARY KEY (grouping_key, position)
) STRICT;

-- What the import list's triggers look a changed row's releases up by.
CREATE INDEX IF NOT EXISTS release_grouping_member_by_folder
    ON release_grouping_member (member_folder);
