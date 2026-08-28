-- Version 2 stored a picked source plus nullable overrides. Version 3 stores
-- one complete editable draft with optional provenance. Those two shapes cannot
-- be translated faithfully in SQL because the old source documents require the
-- Rust provider mappers. Import candidates and scan results are device-local,
-- derived state, so invalidate them while preserving watched folders, skip
-- choices, folder-boundary choices, and every synced library row.

DROP TABLE IF EXISTS import_candidate_track_artist_assignment;
DROP TABLE IF EXISTS import_candidate_track_mapping;
DROP TABLE IF EXISTS import_candidate_track_edit;
DROP TABLE IF EXISTS import_candidate_album_artist_assignment;
DROP TABLE IF EXISTS import_candidate_edit;
DROP TABLE IF EXISTS import_candidate_cover;
DROP TABLE IF EXISTS import_candidate_failure;
DROP TABLE IF EXISTS import_candidate_signal_value;
DROP TABLE IF EXISTS import_candidate_signals;
DROP TABLE IF EXISTS import_candidate_file_duration;
DROP TABLE IF EXISTS import_candidate_file_edit;
DROP TABLE IF EXISTS import_candidate_match;
DROP TABLE IF EXISTS import_candidate_state;

DROP TABLE IF EXISTS folder_scan_directory;
DROP TABLE IF EXISTS scan_candidate_resolved_boundary;
DROP TABLE IF EXISTS scan_cue_index;
DROP TABLE IF EXISTS scan_cue_track;
DROP TABLE IF EXISTS scan_cue_sheet;
DROP TABLE IF EXISTS scan_candidate_file_tag;
DROP TABLE IF EXISTS scan_candidate_tag_snapshot;
DROP TABLE IF EXISTS scan_candidate_file;
DROP TABLE IF EXISTS scan_candidate;
DROP TABLE IF EXISTS folder_scan_roots;
-- Device-local import triage state, keyed by an import candidate's content
-- hash (`CategorizedFiles::content_hash` — sorted (relative_path, size) over
-- every file the release carries). NOT synced: no `_updated_at` and absent from
-- `synced_tables()`, the same device-local convention as `playback_state`
-- above. Adding, removing, or resizing a file changes the hash, which is the
-- invalidation — nothing deletes the orphaned row under the old hash.
--
-- Two independent things share the row, because both are derived state about
-- one set of bytes: what identification concluded, and what the user decided.
-- Either can be present without the other.
CREATE TABLE IF NOT EXISTS import_candidate_state (
    content_hash             TEXT PRIMARY KEY,
    -- Where the candidate was last seen. Not identity, not authoritative.
    folder_path              TEXT NOT NULL,
    -- The identify result. Set and cleared as one group: all NULL together or
    -- all set together. Set only once identification reached a terminal
    -- verdict; cleared when a file decision changes what the folder is.
    verdict_kind             TEXT CHECK (verdict_kind IN ('found', 'not_found', 'manual_only')),
    verdict_track_count      INTEGER CHECK (verdict_track_count IS NULL OR verdict_track_count >= 0),
    -- Which of the candidate's barcodes the lookup that matched ran against.
    -- The barcode rows carry the image each was read off, so this names which
    -- of them is the evidence a chip belongs on. NULL when no barcode matched.
    verdict_matched_barcode  TEXT,
    probed_total_duration_ms INTEGER,
    identified_at            TEXT,
    provenance_kind         TEXT CHECK (provenance_kind IS NULL OR provenance_kind IN ('external_release', 'file_tags')),
    provenance_source       TEXT CHECK (provenance_source IS NULL OR provenance_source IN ('musicbrainz', 'discogs')),
    provenance_release_id   TEXT,
    provenance_author       TEXT CHECK (provenance_author IS NULL OR provenance_author IN ('user', 'identification')),
    -- Advances with every metadata-draft or selected-cover mutation. Commands
    -- return this value so a surface can wait for the exact committed detail.
    metadata_revision        INTEGER NOT NULL DEFAULT 0 CHECK (metadata_revision >= 0),
    -- Advances with every file decision, so a verdict derived from an older
    -- shape is refused.
    edit_revision            INTEGER NOT NULL DEFAULT 0 CHECK (edit_revision >= 0),
    CHECK (
        (verdict_kind IS NULL AND verdict_track_count IS NULL
            AND probed_total_duration_ms IS NULL AND identified_at IS NULL)
        OR
        (verdict_kind IS NOT NULL AND probed_total_duration_ms IS NOT NULL
            AND probed_total_duration_ms >= 0 AND identified_at IS NOT NULL)
    ),
    CHECK (verdict_kind IN ('found', 'manual_only') = (verdict_track_count IS NOT NULL)),
    -- A matched barcode with no found verdict behind it is not a provenance.
    CHECK (verdict_matched_barcode IS NULL OR verdict_kind = 'found'),
    CHECK (
        (provenance_kind IS NULL AND provenance_source IS NULL
            AND provenance_release_id IS NULL AND provenance_author IS NULL)
        OR
        (provenance_kind = 'external_release' AND provenance_source IS NOT NULL
            AND provenance_release_id IS NOT NULL AND provenance_author IS NOT NULL)
        OR
        (provenance_kind = 'file_tags' AND provenance_source IS NULL
            AND provenance_release_id IS NULL AND provenance_author IS NOT NULL)
    ),
    CHECK (provenance_author != 'identification' OR provenance_kind = 'external_release')
) STRICT;

-- One matched release of a found verdict, in match order, with the provenance
-- saying which signal produced it.
CREATE TABLE IF NOT EXISTS import_candidate_match (
    content_hash           TEXT NOT NULL,
    position               INTEGER NOT NULL CHECK (position >= 0),
    source                 TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id             TEXT NOT NULL,
    title                  TEXT NOT NULL,
    artist                 TEXT,
    year                   INTEGER,
    format                 TEXT,
    label                  TEXT,
    catalog_number         TEXT,
    country                TEXT,
    cover_url              TEXT,
    cover_thumbnail_url    TEXT,
    cover_label            TEXT,
    cover_source           TEXT CHECK (cover_source IS NULL OR cover_source IN ('musicbrainz', 'discogs')),
    source_group_id        TEXT,
    -- NULL: nobody asked the source for its tracklist yet. 'listed' /
    -- 'nothing': asked. The total is NULL when any listed track has no length.
    source_tracks_kind     TEXT CHECK (source_tracks_kind IS NULL OR source_tracks_kind IN ('listed', 'nothing')),
    source_tracks_count    INTEGER CHECK (source_tracks_count IS NULL OR source_tracks_count >= 0),
    source_tracks_total_ms INTEGER CHECK (source_tracks_total_ms IS NULL OR source_tracks_total_ms >= 0),
    by_disc_id             INTEGER NOT NULL CHECK (by_disc_id IN (0, 1)),
    by_barcode             INTEGER NOT NULL CHECK (by_barcode IN (0, 1)),
    by_catalog        INTEGER NOT NULL CHECK (by_catalog IN (0, 1)),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((cover_url IS NULL) = (cover_thumbnail_url IS NULL) AND (cover_url IS NULL) = (cover_label IS NULL) AND (cover_url IS NULL) = (cover_source IS NULL)),
    CHECK ((source_tracks_kind = 'listed') = (source_tracks_count IS NOT NULL)),
    CHECK (source_tracks_total_ms IS NULL OR source_tracks_kind = 'listed')
) STRICT;

-- One file's user decisions: its role, which audio a sheet describes, which
-- disc a sheet is. A column is NULL where the user decided nothing about
-- that aspect; an absent row is no decision at all. A cleared sheet binding
-- is stored ('cleared'), not removed: the scan's proposal is not restored.
CREATE TABLE IF NOT EXISTS import_candidate_file_edit (
    content_hash          TEXT NOT NULL,
    relative_path         TEXT NOT NULL,
    role_choice           TEXT CHECK (role_choice IS NULL OR role_choice IN ('audio', 'not_a_track')),
    sheet_binding         TEXT CHECK (sheet_binding IS NULL OR sheet_binding IN ('describes', 'cleared')),
    sheet_binding_file_id TEXT,
    sheet_disc            TEXT CHECK (sheet_disc IS NULL OR sheet_disc IN ('disc', 'ignored')),
    sheet_disc_number     INTEGER CHECK (sheet_disc_number IS NULL OR sheet_disc_number >= 1),
    PRIMARY KEY (content_hash, relative_path),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((sheet_binding = 'describes') = (sheet_binding_file_id IS NOT NULL)),
    CHECK ((sheet_disc = 'disc') = (sheet_disc_number IS NOT NULL)),
    CHECK (role_choice IS NOT NULL OR sheet_binding IS NOT NULL OR sheet_disc IS NOT NULL)
) STRICT;


-- What one of the candidate's audio units plays for, as reading it off the
-- disk found. Written by identification and by the pane when it opens a
-- candidate identification never measured; read by the mapping table, so
-- opening a candidate never opens its files again.
--
-- `duration_ms` NULL means the unit was read and states no length — a file
-- that would not probe, or a sheet entry the sheet gives no timing. The
-- absence of a row is the different fact that nothing has read it yet.
CREATE TABLE IF NOT EXISTS import_candidate_file_duration (
    content_hash        TEXT NOT NULL,
    -- 'file': relative_path is the audio file. 'slice': relative_path is
    -- the container; sheet_relative_path and slice_index name the entry.
    kind                TEXT NOT NULL CHECK (kind IN ('file', 'slice')),
    relative_path       TEXT NOT NULL,
    sheet_relative_path TEXT NOT NULL DEFAULT '',
    -- The sheet's playable tracks counted from zero — the number the binding
    -- carries, not the number the sheet prints.
    slice_index         INTEGER NOT NULL DEFAULT -1 CHECK (slice_index >= -1),
    duration_ms         INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    PRIMARY KEY (content_hash, kind, relative_path, sheet_relative_path, slice_index),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    -- A STRICT primary key column cannot be NULL, so the two sentinels stand
    -- in for "this kind names no sheet entry"; these keep them and the kind
    -- agreeing.
    CHECK ((kind = 'file') = (sheet_relative_path = '' AND slice_index = -1)),
    CHECK ((kind = 'slice') = (sheet_relative_path <> '' AND slice_index >= 0))
) STRICT;

-- The signals identification settled on for one candidate: the disc ID, the
-- barcode verdict, and the classified text, with their list values in
-- `import_candidate_signal_value`. Stored so the pane and the search sheet
-- read them back after a restart instead of re-extracting them.
--
-- Only a settled value is storable: a `Scanning` barcode or text signal is
-- artwork OCR still running, and a verdict is written only after it finishes.
CREATE TABLE IF NOT EXISTS import_candidate_signals (
    content_hash        TEXT PRIMARY KEY,
    disc_id_state       TEXT NOT NULL CHECK (disc_id_state IN ('computed', 'absent', 'failed')),
    disc_id             TEXT,
    -- The candidate-relative path of the LOG or CUE the disc ID came from, so a
    -- surface can put it on that file's row. NULL for a re-identify pass over a
    -- library release, which derives the ID from stored tracks rather than a
    -- file of a scanned folder.
    disc_id_source_file TEXT,
    track_count         INTEGER NOT NULL CHECK (track_count >= 0),
    disc_id_failure     TEXT CHECK (disc_id_failure IS NULL OR disc_id_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    disc_id_failure_status INTEGER,
    disc_id_failure_detail TEXT,
    barcode_state       TEXT NOT NULL CHECK (barcode_state IN ('settled', 'failed', 'absent')),
    barcode_failure     TEXT CHECK (barcode_failure IS NULL OR barcode_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    barcode_failure_status INTEGER,
    barcode_failure_detail TEXT,
    text_state          TEXT NOT NULL CHECK (text_state IN ('settled', 'failed')),
    text_failure        TEXT CHECK (text_failure IS NULL OR text_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    text_failure_status INTEGER,
    text_failure_detail TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((disc_id_state = 'computed') = (disc_id IS NOT NULL)),
    -- A source file with no computed ID behind it is not a provenance.
    CHECK (disc_id_source_file IS NULL OR disc_id_state = 'computed'),
    CHECK ((disc_id_state = 'failed') = (disc_id_failure IS NOT NULL)),
    CHECK ((barcode_state = 'failed') = (barcode_failure IS NOT NULL)),
    CHECK ((text_state = 'failed') = (text_failure IS NOT NULL)),
    CHECK (disc_id_failure_status IS NULL OR disc_id_failure = 'provider'),
    CHECK ((disc_id_failure = 'diagnostic') = (disc_id_failure_detail IS NOT NULL)),
    CHECK (barcode_failure_status IS NULL OR barcode_failure = 'provider'),
    CHECK ((barcode_failure = 'diagnostic') = (barcode_failure_detail IS NOT NULL)),
    CHECK (text_failure_status IS NULL OR text_failure = 'provider'),
    CHECK ((text_failure = 'diagnostic') = (text_failure_detail IS NOT NULL))
) STRICT;

-- One value of one of the three lists a settled signal carries, in the order
-- extraction found it. Free text carries no origin; barcodes and catalog
-- numbers do, because their badges say where each came from.
CREATE TABLE IF NOT EXISTS import_candidate_signal_value (
    content_hash TEXT NOT NULL,
    list         TEXT NOT NULL CHECK (list IN ('barcode', 'catalog', 'free_text')),
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT NOT NULL,
    origin       TEXT CHECK (origin IS NULL OR origin IN ('disc_toc', 'cue_sheet', 'artwork', 'folder_name', 'filename', 'text_file')),
    -- The candidate-relative path of the file the value was read off, where the
    -- origin is a file: the image OCR found a barcode on, the sheet a field came
    -- from. NULL where the origin names no file (the folder's own name), and for
    -- a re-identify pass over a library release, whose images are stored blobs.
    -- Relative because these rows sync, and one device's absolute path means
    -- nothing on another's.
    origin_path  TEXT,
    PRIMARY KEY (content_hash, list, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_signals (content_hash) ON DELETE CASCADE,
    CHECK ((list = 'free_text') = (origin IS NULL)),
    -- A file with no origin behind it is not a provenance.
    CHECK (origin_path IS NULL OR origin IS NOT NULL)
) STRICT;

-- The last import of this candidate that failed, so the pane still offers
-- Retry after a relaunch. Cleared when an import is queued for the hash;
-- written by the worker when one fails.
CREATE TABLE IF NOT EXISTS import_candidate_failure (
    content_hash TEXT PRIMARY KEY,
    error        TEXT NOT NULL,
    failed_at    TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The cover the user chose for this candidate: one of the folder's own images,
-- or one of the picked release's remote covers. No row means the picked
-- release's default cover stands; a row is written only on an explicit choice.
CREATE TABLE IF NOT EXISTS import_candidate_cover (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('local', 'remote')),
    file_id      TEXT,
    url          TEXT,
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((kind = 'local') = (file_id IS NOT NULL)),
    CHECK ((kind = 'remote') = (url IS NOT NULL AND source IS NOT NULL))
) STRICT;

-- The candidate's editable album-level draft. Empty strings are real blank
-- form values; the row exists for every discovered candidate.
CREATE TABLE IF NOT EXISTS import_candidate_edit (
    content_hash      TEXT PRIMARY KEY,
    album_title       TEXT NOT NULL,
    year              TEXT NOT NULL,
    format            TEXT NOT NULL,
    label             TEXT NOT NULL,
    catalog_number    TEXT NOT NULL,
    country           TEXT NOT NULL,
    barcode           TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS import_candidate_album_artist_assignment (
    content_hash          TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('existing', 'new')),
    artist_id             TEXT,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE RESTRICT,
    CHECK (
        (assignment_kind = 'existing' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL)
        OR
        (assignment_kind = 'new' AND artist_id IS NULL AND name IS NOT NULL AND name <> '')
    )
) STRICT;

-- The candidate's editable track metadata. Physical mapping is stored below
-- so replacing this draft cannot delete a file decision.
CREATE TABLE IF NOT EXISTS import_candidate_track_edit (
    content_hash           TEXT NOT NULL,
    track_id               TEXT NOT NULL,
    position               INTEGER NOT NULL CHECK (position >= 0),
    title                  TEXT NOT NULL,
    artist_assignment_kind TEXT NOT NULL CHECK (artist_assignment_kind IN ('album_artists', 'explicit')),
    side                   INTEGER NOT NULL,
    track_number INTEGER,
    PRIMARY KEY (content_hash, track_id),
    UNIQUE (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS import_candidate_track_artist_assignment (
    content_hash          TEXT NOT NULL,
    track_id              TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('existing', 'new')),
    artist_id             TEXT,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (content_hash, track_id, position),
    FOREIGN KEY (content_hash, track_id)
        REFERENCES import_candidate_track_edit (content_hash, track_id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE RESTRICT,
    CHECK (
        (assignment_kind = 'existing' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL)
        OR
        (assignment_kind = 'new' AND artist_id IS NULL AND name IS NOT NULL AND name <> '')
    )
) STRICT;

CREATE TABLE IF NOT EXISTS import_candidate_track_mapping (
    content_hash TEXT NOT NULL,
    track_id     TEXT NOT NULL,
    dropped      INTEGER NOT NULL CHECK (dropped IN (0, 1)),
    file_kind    TEXT CHECK (file_kind IS NULL OR file_kind IN ('standalone', 'sheet_slice')),
    file_id      TEXT,
    sheet_id     TEXT,
    slice_index  INTEGER CHECK (slice_index IS NULL OR slice_index >= 0),
    PRIMARY KEY (content_hash, track_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK (dropped = 0 OR file_kind IS NULL),
    CHECK ((file_kind IS NOT NULL) = (file_id IS NOT NULL)),
    CHECK ((file_kind = 'sheet_slice') = (sheet_id IS NOT NULL AND slice_index IS NOT NULL))
) STRICT;

CREATE TABLE IF NOT EXISTS folder_scan_roots (
    watched_folder_path TEXT PRIMARY KEY,
    generation          INTEGER NOT NULL CHECK (generation >= 0),
    status              TEXT NOT NULL CHECK (status IN ('scanning', 'complete', 'failed')),
    error               TEXT,
    CHECK (
        (status = 'failed' AND error IS NOT NULL)
        OR
        (status != 'failed' AND error IS NULL)
    ),
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path)
        ON DELETE CASCADE
) STRICT;


-- One scanned folder under a watched root. `kind` is what the scan made of
-- it: a release approximation seen before its enclosing boundary was known
-- (tentative), a release (valid), or a folder that failed validation
-- (invalid — carries the reason, no files). Entries are written as they are
-- discovered; successful completion removes entries not seen in that
-- generation in the same transaction that marks the root complete.
CREATE TABLE IF NOT EXISTS scan_candidate (
    watched_folder_path            TEXT NOT NULL,
    path                           TEXT NOT NULL,
    generation                     INTEGER NOT NULL CHECK (generation >= 0),
    kind                           TEXT NOT NULL CHECK (kind IN ('tentative', 'valid', 'invalid')),
    name                           TEXT NOT NULL,
    display_path                   TEXT NOT NULL,
    file_root                      TEXT,
    scope                          TEXT CHECK (scope IS NULL OR scope IN ('direct', 'recursive')),
    content_hash                   TEXT,
    file_edit_revision             INTEGER NOT NULL DEFAULT 0 CHECK (file_edit_revision >= 0),
    format_label                   TEXT,
    initial_metadata_source       TEXT CHECK (initial_metadata_source IS NULL OR initial_metadata_source IN ('find_online', 'file_tags', 'none')),
    combine_ancestor_relative_path TEXT,
    invalid_reason                 TEXT CHECK (invalid_reason IS NULL OR invalid_reason IN ('corrupt_audio', 'corrupt_image', 'no_valid_audio')),
    invalid_reason_path            TEXT,
    PRIMARY KEY (watched_folder_path, path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE,
    CHECK ((kind = 'invalid') = (invalid_reason IS NOT NULL)),
    CHECK ((kind = 'invalid') = (file_root IS NULL AND scope IS NULL AND content_hash IS NULL AND format_label IS NULL AND initial_metadata_source IS NULL)),
    CHECK ((invalid_reason IN ('corrupt_audio', 'corrupt_image')) = (invalid_reason_path IS NOT NULL))
) STRICT;

-- Every file under a candidate's root, in relative_path order, with the
-- role the scan proposed (and the user's decisions applied). A track sheet
-- carries what its FILE directive resolved to and which disc it is.
CREATE TABLE IF NOT EXISTS scan_candidate_file (
    watched_folder_path   TEXT NOT NULL,
    candidate_path        TEXT NOT NULL,
    relative_path         TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    absolute_path         TEXT NOT NULL,
    size                  INTEGER NOT NULL CHECK (size >= 0),
    file_name             TEXT NOT NULL,
    dir_prefix            TEXT,
    proposed_audio        INTEGER NOT NULL CHECK (proposed_audio IN (0, 1)),
    role                  TEXT NOT NULL CHECK (role IN ('audio', 'track_sheet', 'artwork', 'document', 'other')),
    sheet_binding         TEXT CHECK (sheet_binding IS NULL OR sheet_binding IN ('describes', 'unresolved', 'refused_codec')),
    sheet_binding_file_id TEXT,
    sheet_binding_codec   TEXT,
    sheet_disc            TEXT CHECK (sheet_disc IS NULL OR sheet_disc IN ('disc', 'ignored')),
    sheet_disc_number     INTEGER CHECK (sheet_disc_number IS NULL OR sheet_disc_number >= 1),
    PRIMARY KEY (watched_folder_path, candidate_path, relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path) REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE,
    CHECK ((role = 'track_sheet') = (sheet_binding IS NOT NULL AND sheet_disc IS NOT NULL)),
    CHECK ((sheet_binding IN ('describes', 'refused_codec')) = (sheet_binding_file_id IS NOT NULL)),
    CHECK ((sheet_binding = 'refused_codec') = (sheet_binding_codec IS NOT NULL)),
    CHECK ((sheet_disc = 'disc') = (sheet_disc_number IS NOT NULL))
) STRICT;

-- Complete file-tag readings belong to one persisted scan candidate. The
-- header, file facts, and metadata draft are replaced atomically after the
-- candidate's generation and file-edit revision are verified.
CREATE TABLE scan_candidate_tag_snapshot (
    watched_folder_path                 TEXT NOT NULL,
    candidate_path                      TEXT NOT NULL,
    scan_generation                     INTEGER NOT NULL CHECK (scan_generation >= 0),
    file_edit_revision                  INTEGER NOT NULL CHECK (file_edit_revision >= 0),
    embedded_cover_source_relative_path TEXT,
    embedded_cover_content_type         TEXT,
    embedded_cover_data                 BLOB,
    PRIMARY KEY (watched_folder_path, candidate_path),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE,
    CHECK (
        (embedded_cover_source_relative_path IS NULL
            AND embedded_cover_content_type IS NULL AND embedded_cover_data IS NULL)
        OR
        (embedded_cover_source_relative_path IS NOT NULL
            AND embedded_cover_content_type IS NOT NULL AND embedded_cover_data IS NOT NULL)
    )
) STRICT;

CREATE TABLE scan_candidate_file_tag (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    relative_path       TEXT NOT NULL,
    file_size           INTEGER NOT NULL CHECK (file_size >= 0),
    modified_at_ns      INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    content_type        TEXT,
    title               TEXT,
    track_artist        TEXT,
    album_title         TEXT,
    album_artist        TEXT,
    year                INTEGER,
    track_number        INTEGER,
    disc_number         INTEGER,
    PRIMARY KEY (watched_folder_path, candidate_path, relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate_tag_snapshot (watched_folder_path, candidate_path) ON DELETE CASCADE,
    FOREIGN KEY (watched_folder_path, candidate_path, relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE
) STRICT;

-- The parsed sheet behind a track-sheet file.
CREATE TABLE IF NOT EXISTS scan_cue_sheet (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    sheet_relative_path TEXT NOT NULL,
    title               TEXT,
    performer           TEXT,
    catalog             TEXT,
    date                TEXT,
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS scan_cue_track (
    watched_folder_path         TEXT NOT NULL,
    candidate_path              TEXT NOT NULL,
    sheet_relative_path         TEXT NOT NULL,
    position                    INTEGER NOT NULL CHECK (position >= 0),
    number                      INTEGER NOT NULL,
    mode                        TEXT NOT NULL CHECK (mode IN ('audio', 'other')),
    mode_other                  TEXT,
    title                       TEXT,
    performer                   TEXT,
    file_reference              TEXT NOT NULL,
    start_cue_frames            INTEGER NOT NULL CHECK (start_cue_frames >= 0),
    end_cue_frames              INTEGER CHECK (end_cue_frames IS NULL OR end_cue_frames >= 0),
    pregap_kind                 TEXT NOT NULL CHECK (pregap_kind IN ('none', 'audio', 'silence')),
    pregap_frames               INTEGER CHECK (pregap_frames IS NULL OR pregap_frames >= 0),
    pregap_index_number         INTEGER,
    pregap_index_file_reference TEXT,
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path, position),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_cue_sheet (watched_folder_path, candidate_path, sheet_relative_path) ON DELETE CASCADE,
    CHECK ((mode = 'other') = (mode_other IS NOT NULL)),
    CHECK ((pregap_kind = 'none') = (pregap_frames IS NULL)),
    CHECK ((pregap_kind = 'audio') = (pregap_index_number IS NOT NULL AND pregap_index_file_reference IS NOT NULL))
) STRICT;

CREATE TABLE IF NOT EXISTS scan_cue_index (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    sheet_relative_path TEXT NOT NULL,
    track_position      INTEGER NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    number              INTEGER NOT NULL,
    frames              INTEGER NOT NULL CHECK (frames >= 0),
    file_reference      TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path, track_position, position),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path, track_position)
        REFERENCES scan_cue_track (watched_folder_path, candidate_path, sheet_relative_path, position) ON DELETE CASCADE
) STRICT;

-- The explicit boundary decisions that exposed a candidate, retained so its
-- context menu can set the opposite interpretation.
CREATE TABLE IF NOT EXISTS scan_candidate_resolved_boundary (
    watched_folder_path  TEXT NOT NULL,
    candidate_path       TEXT NOT NULL,
    position             INTEGER NOT NULL CHECK (position >= 0),
    relative_folder_path TEXT NOT NULL,
    decision             TEXT NOT NULL CHECK (decision IN ('combine_as_one_release', 'keep_as_separate_releases')),
    name                 TEXT NOT NULL,
    display_path         TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, position),
    FOREIGN KEY (watched_folder_path, candidate_path) REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE
) STRICT;

-- Every directory a completed walk of this root read, and when it was last
-- modified. What lets a folder on a network volume be asked "has anything
-- moved?" without walking it: a directory's mtime changes when a file in it is
-- added, removed or rewritten, and adding a folder changes its parent's.
--
-- Recorded only by a walk that could read the mtime of every directory it
-- visited. A root with no rows here is a root nothing can be concluded about,
-- and it is walked.
CREATE TABLE IF NOT EXISTS folder_scan_directory (
    watched_folder_path TEXT NOT NULL,
    path                TEXT NOT NULL,
    modified_at         INTEGER NOT NULL,
    PRIMARY KEY (watched_folder_path, path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE
) STRICT;

-- A candidate is addressed by its path alone (the key a selection carries)
-- and gathered by its content hash (the file decision that reshapes every
-- copy of one release). Neither is the leading column of the primary key.
CREATE INDEX IF NOT EXISTS idx_scan_candidate_path ON scan_candidate (path);
CREATE INDEX IF NOT EXISTS idx_scan_candidate_content_hash
    ON scan_candidate (content_hash) WHERE content_hash IS NOT NULL;

-- metadata_source is part of the synced positional release row. Keep the
-- column and encode absent provenance as none; Rust exposes it as Option::None.
CREATE TABLE release_metadata_provenance_validation (
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;

INSERT INTO release_metadata_provenance_validation (valid)
SELECT CASE WHEN COUNT(*) = 0 THEN 1 ELSE 0 END
FROM releases
WHERE NOT (
    (metadata_source IN ('musicbrainz', 'discogs')
        AND metadata_source_release_id IS NOT NULL)
    OR (metadata_source IN ('file_tags', 'none')
        AND metadata_source_release_id IS NULL)
);

DROP TABLE release_metadata_provenance_validation;

CREATE TRIGGER releases_metadata_provenance_insert
BEFORE INSERT ON releases
WHEN NOT (
    (NEW.metadata_source IN ('musicbrainz', 'discogs')
        AND NEW.metadata_source_release_id IS NOT NULL)
    OR (NEW.metadata_source IN ('file_tags', 'none')
        AND NEW.metadata_source_release_id IS NULL)
)
BEGIN
    SELECT RAISE(ABORT, 'invalid releases metadata provenance');
END;

CREATE TRIGGER releases_metadata_provenance_update
BEFORE UPDATE OF metadata_source, metadata_source_release_id ON releases
WHEN NOT (
    (NEW.metadata_source IN ('musicbrainz', 'discogs')
        AND NEW.metadata_source_release_id IS NOT NULL)
    OR (NEW.metadata_source IN ('file_tags', 'none')
        AND NEW.metadata_source_release_id IS NULL)
)
BEGIN
    SELECT RAISE(ABORT, 'invalid releases metadata provenance');
END;
