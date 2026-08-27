-- Import candidates now store the metadata seed that will be committed, not
-- an identity-shaped proxy. Rebuild the candidate graph so every foreign key
-- points at the new parent and version-1 artist text can be normalized by the
-- Rust migration step before its staging tables are dropped.

ALTER TABLE import_candidate_signal_value RENAME TO import_candidate_signal_value_v1;
ALTER TABLE import_candidate_match RENAME TO import_candidate_match_v1;
ALTER TABLE import_candidate_file_edit RENAME TO import_candidate_file_edit_v1;
ALTER TABLE import_candidate_file_duration RENAME TO import_candidate_file_duration_v1;
ALTER TABLE import_candidate_failure RENAME TO import_candidate_failure_v1;
ALTER TABLE import_candidate_cover RENAME TO import_candidate_cover_v1;
ALTER TABLE import_candidate_edit RENAME TO import_candidate_edit_v1;
ALTER TABLE import_candidate_track_edit RENAME TO import_candidate_track_edit_v1;
ALTER TABLE import_candidate_signals RENAME TO import_candidate_signals_v1;
ALTER TABLE import_candidate_state RENAME TO import_candidate_state_v1;

CREATE TABLE import_candidate_state (
    content_hash             TEXT PRIMARY KEY,
    folder_path              TEXT NOT NULL,
    verdict_kind             TEXT CHECK (verdict_kind IN ('found', 'not_found', 'manual_only')),
    verdict_track_count      INTEGER CHECK (verdict_track_count IS NULL OR verdict_track_count >= 0),
    verdict_matched_barcode  TEXT,
    probed_total_duration_ms INTEGER,
    identified_at            TEXT,
    seed_kind                TEXT CHECK (seed_kind IS NULL OR seed_kind IN ('external_release', 'file_tags', 'manual')),
    seed_source              TEXT CHECK (seed_source IS NULL OR seed_source IN ('musicbrainz', 'discogs')),
    seed_release_id          TEXT,
    metadata_seed_author     TEXT CHECK (metadata_seed_author IS NULL OR metadata_seed_author IN ('user', 'identification')),
    edit_revision            INTEGER NOT NULL DEFAULT 0 CHECK (edit_revision >= 0),
    CHECK (
        (verdict_kind IS NULL AND verdict_track_count IS NULL
            AND probed_total_duration_ms IS NULL AND identified_at IS NULL)
        OR
        (verdict_kind IS NOT NULL AND probed_total_duration_ms IS NOT NULL
            AND probed_total_duration_ms >= 0 AND identified_at IS NOT NULL)
    ),
    CHECK (verdict_kind IN ('found', 'manual_only') = (verdict_track_count IS NOT NULL)),
    CHECK (verdict_matched_barcode IS NULL OR verdict_kind = 'found'),
    CHECK (
        (seed_kind IS NULL AND seed_source IS NULL AND seed_release_id IS NULL
            AND metadata_seed_author IS NULL)
        OR
        (seed_kind = 'external_release' AND seed_source IS NOT NULL
            AND seed_release_id IS NOT NULL AND metadata_seed_author IS NOT NULL)
        OR
        (seed_kind IN ('file_tags', 'manual') AND seed_source IS NULL
            AND seed_release_id IS NULL AND metadata_seed_author = 'user')
    ),
    CHECK (metadata_seed_author != 'identification' OR seed_kind = 'external_release')
) STRICT;

INSERT INTO import_candidate_state (
    content_hash, folder_path, verdict_kind, verdict_track_count,
    verdict_matched_barcode, probed_total_duration_ms, identified_at,
    seed_kind, seed_source, seed_release_id, metadata_seed_author, edit_revision
)
SELECT
    content_hash, folder_path, verdict_kind, verdict_track_count,
    verdict_matched_barcode, probed_total_duration_ms, identified_at,
    CASE pick_kind
        WHEN 'release' THEN 'external_release'
        WHEN 'unknown' THEN 'file_tags'
        ELSE NULL
    END,
    CASE WHEN pick_kind = 'release' THEN pick_source ELSE NULL END,
    CASE WHEN pick_kind = 'release' THEN pick_release_id ELSE NULL END,
    identity_pick_author,
    edit_revision
FROM import_candidate_state_v1;

CREATE TABLE import_candidate_match (
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
    source_tracks_kind     TEXT CHECK (source_tracks_kind IS NULL OR source_tracks_kind IN ('listed', 'nothing')),
    source_tracks_count    INTEGER CHECK (source_tracks_count IS NULL OR source_tracks_count >= 0),
    source_tracks_total_ms INTEGER CHECK (source_tracks_total_ms IS NULL OR source_tracks_total_ms >= 0),
    by_disc_id             INTEGER NOT NULL CHECK (by_disc_id IN (0, 1)),
    by_barcode             INTEGER NOT NULL CHECK (by_barcode IN (0, 1)),
    by_catalog             INTEGER NOT NULL CHECK (by_catalog IN (0, 1)),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((cover_url IS NULL) = (cover_thumbnail_url IS NULL)
        AND (cover_url IS NULL) = (cover_label IS NULL)
        AND (cover_url IS NULL) = (cover_source IS NULL)),
    CHECK ((source_tracks_kind = 'listed') = (source_tracks_count IS NOT NULL)),
    CHECK (source_tracks_total_ms IS NULL OR source_tracks_kind = 'listed')
) STRICT;

INSERT INTO import_candidate_match SELECT * FROM import_candidate_match_v1;

CREATE TABLE import_candidate_file_edit (
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

INSERT INTO import_candidate_file_edit SELECT * FROM import_candidate_file_edit_v1;

CREATE TABLE import_candidate_file_duration (
    content_hash        TEXT NOT NULL,
    kind                TEXT NOT NULL CHECK (kind IN ('file', 'slice')),
    relative_path       TEXT NOT NULL,
    sheet_relative_path TEXT NOT NULL DEFAULT '',
    slice_index         INTEGER NOT NULL DEFAULT -1 CHECK (slice_index >= -1),
    duration_ms         INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    PRIMARY KEY (content_hash, kind, relative_path, sheet_relative_path, slice_index),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((kind = 'file') = (sheet_relative_path = '' AND slice_index = -1)),
    CHECK ((kind = 'slice') = (sheet_relative_path <> '' AND slice_index >= 0))
) STRICT;

INSERT INTO import_candidate_file_duration SELECT * FROM import_candidate_file_duration_v1;

CREATE TABLE import_candidate_signals (
    content_hash        TEXT PRIMARY KEY,
    disc_id_state       TEXT NOT NULL CHECK (disc_id_state IN ('computed', 'absent', 'failed')),
    disc_id             TEXT,
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

INSERT INTO import_candidate_signals SELECT * FROM import_candidate_signals_v1;

CREATE TABLE import_candidate_signal_value (
    content_hash TEXT NOT NULL,
    list         TEXT NOT NULL CHECK (list IN ('barcode', 'catalog', 'free_text')),
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT NOT NULL,
    origin       TEXT CHECK (origin IS NULL OR origin IN ('disc_toc', 'cue_sheet', 'artwork', 'folder_name', 'filename', 'text_file')),
    origin_path  TEXT,
    PRIMARY KEY (content_hash, list, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_signals (content_hash) ON DELETE CASCADE,
    CHECK ((list = 'free_text') = (origin IS NULL)),
    CHECK (origin_path IS NULL OR origin IS NOT NULL)
) STRICT;

INSERT INTO import_candidate_signal_value SELECT * FROM import_candidate_signal_value_v1;

CREATE TABLE import_candidate_failure (
    content_hash TEXT PRIMARY KEY,
    error        TEXT NOT NULL,
    failed_at    TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_failure SELECT * FROM import_candidate_failure_v1;

CREATE TABLE import_candidate_cover (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('local', 'remote')),
    file_id      TEXT,
    url          TEXT,
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((kind = 'local') = (file_id IS NOT NULL)),
    CHECK ((kind = 'remote') = (url IS NOT NULL AND source IS NOT NULL))
) STRICT;

INSERT INTO import_candidate_cover SELECT * FROM import_candidate_cover_v1;

CREATE TABLE import_candidate_edit (
    content_hash   TEXT PRIMARY KEY,
    album_title    TEXT,
    year           TEXT,
    format         TEXT,
    label          TEXT,
    catalog_number TEXT,
    country        TEXT,
    barcode        TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK (album_title IS NOT NULL OR year IS NOT NULL OR format IS NOT NULL
        OR label IS NOT NULL OR catalog_number IS NOT NULL OR country IS NOT NULL
        OR barcode IS NOT NULL)
) STRICT;

INSERT INTO import_candidate_edit (
    content_hash, album_title, year, format, label, catalog_number, country, barcode
)
SELECT content_hash, album_title, year, format, label, catalog_number, country, barcode
FROM import_candidate_edit_v1
WHERE album_title IS NOT NULL OR year IS NOT NULL OR format IS NOT NULL
    OR label IS NOT NULL OR catalog_number IS NOT NULL OR country IS NOT NULL
    OR barcode IS NOT NULL;

CREATE TABLE import_candidate_album_artist_assignment (
    content_hash          TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('existing', 'new')),
    artist_id             TEXT,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK (
        (assignment_kind = 'existing' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL)
        OR
        (assignment_kind = 'new' AND artist_id IS NULL AND name IS NOT NULL AND name <> '')
    )
) STRICT;

CREATE TABLE import_candidate_track_edit (
    content_hash           TEXT NOT NULL,
    track_id               TEXT NOT NULL,
    dropped                INTEGER NOT NULL DEFAULT 0 CHECK (dropped IN (0, 1)),
    title                  TEXT,
    artist_assignment_kind TEXT CHECK (artist_assignment_kind IS NULL OR artist_assignment_kind IN ('album_artists', 'explicit')),
    side                   INTEGER,
    track_number           INTEGER,
    file_kind              TEXT CHECK (file_kind IS NULL OR file_kind IN ('standalone', 'sheet_slice')),
    file_id                TEXT,
    sheet_id               TEXT,
    slice_index            INTEGER CHECK (slice_index IS NULL OR slice_index >= 0),
    PRIMARY KEY (content_hash, track_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK (
        (dropped = 1 AND title IS NULL AND artist_assignment_kind IS NULL AND side IS NULL
            AND track_number IS NULL AND file_kind IS NULL)
        OR
        (dropped = 0 AND title IS NOT NULL AND artist_assignment_kind IS NOT NULL
            AND side IS NOT NULL)
    ),
    CHECK ((file_kind IS NOT NULL) = (file_id IS NOT NULL)),
    CHECK ((file_kind = 'sheet_slice') = (sheet_id IS NOT NULL AND slice_index IS NOT NULL))
) STRICT;

INSERT INTO import_candidate_track_edit (
    content_hash, track_id, dropped, title, artist_assignment_kind, side,
    track_number, file_kind, file_id, sheet_id, slice_index
)
SELECT
    content_hash, track_id, dropped, title,
    CASE
        WHEN dropped = 1 THEN NULL
        WHEN trim(artist_text) = '' THEN 'album_artists'
        ELSE 'explicit'
    END,
    side, track_number, file_kind, file_id, sheet_id, slice_index
FROM import_candidate_track_edit_v1;

CREATE TABLE import_candidate_track_artist_assignment (
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
    CHECK (
        (assignment_kind = 'existing' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL)
        OR
        (assignment_kind = 'new' AND artist_id IS NULL AND name IS NOT NULL AND name <> '')
    )
) STRICT;

CREATE TABLE scan_candidate_tag_snapshot (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    scan_generation     INTEGER NOT NULL CHECK (scan_generation >= 0),
    file_edit_revision  INTEGER NOT NULL CHECK (file_edit_revision >= 0),
    PRIMARY KEY (watched_folder_path, candidate_path),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE
) STRICT;

CREATE TABLE scan_candidate_file_tag (
    watched_folder_path TEXT NOT NULL,
    candidate_path      TEXT NOT NULL,
    relative_path       TEXT NOT NULL,
    file_size           INTEGER NOT NULL CHECK (file_size >= 0),
    modified_at_ns      INTEGER NOT NULL,
    content_type        TEXT NOT NULL,
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
