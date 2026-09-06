-- One row held three unrelated things: the candidate itself, what
-- identification concluded about it, and where its draft came from.
--
-- The verdict was a group of nullable columns tied together by CHECKs, and it
-- was spread over two tables — the normal outcomes here, a failed one in
-- `import_candidate_identify_failure` — with nothing stopping a candidate from
-- holding both, so every reader refused that pair at runtime instead. The
-- match rows hung off the candidate rather than off the verdict that found
-- them, so clearing a verdict had to remember to clear them too.
--
-- The provenance was a second such group, plus an author column checked
-- against it, sitting on the candidate rather than on the draft it describes —
-- so replacing the draft left them behind to be rewritten by hand.
--
-- Each becomes a row of its own. `import_candidate_verdict` holds one whole
-- verdict, failed included, and the matches hang off it: a candidate cannot
-- hold two verdicts, a match cannot outlive the verdict that found it, and
-- clearing a verdict is one delete. `import_candidate_draft_provenance` holds
-- the provenance and its author together and hangs off the draft, so the two
-- go with the draft they explain. `import_candidate_state` is left with the
-- candidate: its hash, where it was last seen, and the two revisions every
-- write is checked against.
--
-- SQLite cannot drop a column a CHECK names, and cannot repoint a foreign key,
-- so the candidate's rows are rebuilt. The old table is renamed out of the way,
-- which points every child at the renamed table; each child is rebuilt against
-- the new one; the old table is dropped once nothing refers to it. A table is
-- dropped only after its own children have moved off it, because with foreign
-- keys on, dropping a parent cascades its children's rows away.

ALTER TABLE import_candidate_state RENAME TO import_candidate_state_v3;

-- Device-local import triage state, keyed by an import candidate's content
-- hash (`CategorizedFiles::content_hash` — sorted (relative_path, size) over
-- every file the release carries). NOT synced: no `_updated_at` and absent from
-- `synced_tables()`. Adding, removing, or resizing a file changes the hash,
-- which is the invalidation — nothing deletes the orphaned row under the old
-- hash.
CREATE TABLE import_candidate_state (
    content_hash      TEXT PRIMARY KEY,
    -- Where the candidate was last seen. Not identity, not authoritative.
    folder_path       TEXT NOT NULL,
    -- Advances with every metadata-draft or selected-cover mutation. Commands
    -- return this value so a surface can wait for the exact committed detail.
    metadata_revision INTEGER NOT NULL DEFAULT 0 CHECK (metadata_revision >= 0),
    -- Advances with every file decision, so a verdict derived from an older
    -- shape is refused.
    edit_revision     INTEGER NOT NULL DEFAULT 0 CHECK (edit_revision >= 0)
) STRICT;

INSERT INTO import_candidate_state (content_hash, folder_path, metadata_revision, edit_revision)
SELECT content_hash, folder_path, metadata_revision, edit_revision
FROM import_candidate_state_v3;

-- What identification concluded, whole. Present as a row or absent entirely:
-- written once identification reaches a terminal outcome, deleted when a file
-- decision changes what the folder is.
CREATE TABLE import_candidate_verdict (
    content_hash             TEXT PRIMARY KEY,
    kind                     TEXT NOT NULL
        CHECK (kind IN ('found', 'not_found', 'manual_only', 'failed')),
    -- The tracks the folder played when the verdict was reached. Only a
    -- verdict that found nothing anywhere counts none.
    track_count              INTEGER CHECK (track_count IS NULL OR track_count >= 0),
    -- Which of the candidate's barcodes the lookup that matched ran against.
    -- The barcode rows carry the image each was read off, so this names which
    -- of them is the evidence a chip belongs on. NULL when no barcode matched.
    matched_barcode          TEXT,
    -- The typed lookup failures of a failed verdict, serialized as one value
    -- because no query dispatches on their internals; queue placement needs
    -- only the verdict's kind.
    failures_json            TEXT CHECK (
        failures_json IS NULL
        OR (json_valid(failures_json)
            AND json_type(failures_json) = 'array'
            AND json_array_length(failures_json) > 0)
    ),
    probed_total_duration_ms INTEGER NOT NULL CHECK (probed_total_duration_ms >= 0),
    identified_at            TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((kind = 'not_found') = (track_count IS NULL)),
    -- A matched barcode with no found verdict behind it is not a provenance.
    CHECK (matched_barcode IS NULL OR kind = 'found'),
    CHECK ((kind = 'failed') = (failures_json IS NOT NULL))
) STRICT;

INSERT INTO import_candidate_verdict (
    content_hash, kind, track_count, matched_barcode, failures_json,
    probed_total_duration_ms, identified_at
)
SELECT content_hash, verdict_kind, verdict_track_count, verdict_matched_barcode, NULL,
       probed_total_duration_ms, identified_at
FROM import_candidate_state_v3
WHERE verdict_kind IS NOT NULL;

-- A candidate holding a normal verdict and a failed one at once was unreadable:
-- every reader refused the pair rather than choosing between them. The normal
-- verdict is what comes across; the failed half of a contradiction does not.
INSERT INTO import_candidate_verdict (
    content_hash, kind, track_count, matched_barcode, failures_json,
    probed_total_duration_ms, identified_at
)
SELECT content_hash, 'failed', track_count, NULL, failures_json,
       probed_total_duration_ms, identified_at
FROM import_candidate_identify_failure
WHERE content_hash NOT IN (SELECT content_hash FROM import_candidate_verdict);

DROP TABLE import_candidate_identify_failure;

-- One matched release of a found verdict, in match order, with the provenance
-- saying which signal produced it. It hangs off the verdict now, so a match
-- without one is unrepresentable and clearing the verdict clears them.
CREATE TABLE import_candidate_match_v2 (
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
    barcode                TEXT,
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
    by_catalog             INTEGER NOT NULL CHECK (by_catalog IN (0, 1)),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_verdict (content_hash) ON DELETE CASCADE,
    CHECK ((cover_url IS NULL) = (cover_thumbnail_url IS NULL) AND (cover_url IS NULL) = (cover_label IS NULL) AND (cover_url IS NULL) = (cover_source IS NULL)),
    CHECK ((source_tracks_kind = 'listed') = (source_tracks_count IS NOT NULL)),
    CHECK (source_tracks_total_ms IS NULL OR source_tracks_kind = 'listed')
) STRICT;

-- A match under a candidate whose verdict found nothing was already unread:
-- the reader rebuilds a match list only for a found verdict.
INSERT INTO import_candidate_match_v2 (
    content_hash, position, source, release_id, title, artist, year, format, label,
    catalog_number, country, barcode, cover_url, cover_thumbnail_url, cover_label,
    cover_source, source_group_id, source_tracks_kind, source_tracks_count,
    source_tracks_total_ms, by_disc_id, by_barcode, by_catalog
)
SELECT content_hash, position, source, release_id, title, artist, year, format, label,
       catalog_number, country, barcode, cover_url, cover_thumbnail_url, cover_label,
       cover_source, source_group_id, source_tracks_kind, source_tracks_count,
       source_tracks_total_ms, by_disc_id, by_barcode, by_catalog
FROM import_candidate_match
WHERE content_hash IN (SELECT content_hash FROM import_candidate_verdict WHERE kind = 'found');

DROP TABLE import_candidate_match;
ALTER TABLE import_candidate_match_v2 RENAME TO import_candidate_match;

-- The draft and everything hanging off it move to the new candidate row.
ALTER TABLE import_candidate_edit RENAME TO import_candidate_edit_v2;

-- The candidate's editable album-level draft. Empty strings are real blank
-- form values; the row exists for every discovered candidate.
CREATE TABLE import_candidate_edit (
    content_hash   TEXT PRIMARY KEY,
    album_title    TEXT NOT NULL,
    album_year     TEXT NOT NULL,
    year           TEXT NOT NULL,
    format         TEXT NOT NULL,
    label          TEXT NOT NULL,
    catalog_number TEXT NOT NULL,
    country        TEXT NOT NULL,
    barcode        TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_edit (
    content_hash, album_title, album_year, year, format, label, catalog_number, country, barcode
)
SELECT content_hash, album_title, album_year, year, format, label, catalog_number, country, barcode
FROM import_candidate_edit_v2;

ALTER TABLE import_candidate_track RENAME TO import_candidate_track_v2;

-- A candidate draft has exactly one track per audio slot, and every track has
-- exactly one physical decision: which file plays it, who chose that file,
-- whether the source named the track, and whether it is still in the import.
CREATE TABLE import_candidate_track (
    content_hash           TEXT NOT NULL,
    track_id               TEXT NOT NULL,
    position               INTEGER NOT NULL CHECK (position >= 0),
    title                  TEXT NOT NULL,
    artist_assignment_kind TEXT NOT NULL CHECK (artist_assignment_kind IN ('album_artists', 'explicit')),
    side                   INTEGER NOT NULL,
    track_number           INTEGER,
    named_by_source        INTEGER NOT NULL CHECK (named_by_source IN (0, 1)),
    dropped                INTEGER NOT NULL CHECK (dropped IN (0, 1)),
    file_author            TEXT NOT NULL CHECK (file_author IN ('automatic', 'user')),
    file_kind              TEXT CHECK (file_kind IS NULL OR file_kind IN ('standalone', 'sheet_slice')),
    file_id                TEXT,
    sheet_id               TEXT,
    slice_index            INTEGER CHECK (slice_index IS NULL OR slice_index >= 0),
    PRIMARY KEY (content_hash, track_id),
    UNIQUE (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    -- A row out of the import plays nothing, so it names nothing.
    CHECK (dropped = 0 OR file_kind IS NULL),
    CHECK (
        (file_kind IS NULL AND file_id IS NULL AND sheet_id IS NULL AND slice_index IS NULL)
        OR (file_kind = 'standalone' AND file_id IS NOT NULL AND sheet_id IS NULL AND slice_index IS NULL)
        OR (file_kind = 'sheet_slice' AND file_id IS NOT NULL AND sheet_id IS NOT NULL AND slice_index IS NOT NULL)
    )
) STRICT;

INSERT INTO import_candidate_track (
    content_hash, track_id, position, title, artist_assignment_kind, side, track_number,
    named_by_source, dropped, file_author, file_kind, file_id, sheet_id, slice_index
)
SELECT content_hash, track_id, position, title, artist_assignment_kind, side, track_number,
       named_by_source, dropped, file_author, file_kind, file_id, sheet_id, slice_index
FROM import_candidate_track_v2;

CREATE TABLE import_candidate_track_artist_assignment_v2 (
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
        REFERENCES import_candidate_track (content_hash, track_id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE RESTRICT,
    CHECK (
        (assignment_kind = 'existing' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL)
        OR
        (assignment_kind = 'new' AND artist_id IS NULL AND name IS NOT NULL AND name <> '')
    )
) STRICT;

INSERT INTO import_candidate_track_artist_assignment_v2 (
    content_hash, track_id, position, assignment_kind, artist_id, name, sort_name,
    musicbrainz_artist_id, discogs_artist_id
)
SELECT content_hash, track_id, position, assignment_kind, artist_id, name, sort_name,
       musicbrainz_artist_id, discogs_artist_id
FROM import_candidate_track_artist_assignment;

DROP TABLE import_candidate_track_artist_assignment;
ALTER TABLE import_candidate_track_artist_assignment_v2
    RENAME TO import_candidate_track_artist_assignment;

DROP TABLE import_candidate_track_v2;

CREATE TABLE import_candidate_album_artist_assignment_v2 (
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

INSERT INTO import_candidate_album_artist_assignment_v2 (
    content_hash, position, assignment_kind, artist_id, name, sort_name,
    musicbrainz_artist_id, discogs_artist_id
)
SELECT content_hash, position, assignment_kind, artist_id, name, sort_name,
       musicbrainz_artist_id, discogs_artist_id
FROM import_candidate_album_artist_assignment;

DROP TABLE import_candidate_album_artist_assignment;
ALTER TABLE import_candidate_album_artist_assignment_v2
    RENAME TO import_candidate_album_artist_assignment;

DROP TABLE import_candidate_edit_v2;

-- Where the draft came from and who put it there, as one row of the draft's.
-- A draft nobody chose — the blank one discovery creates, or one a person
-- cleared — has no row, so an author without a provenance and a provenance
-- without an author are both unrepresentable. Replacing the draft takes these
-- with it.
CREATE TABLE import_candidate_draft_provenance (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('external_release', 'file_tags')),
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    release_id   TEXT,
    author       TEXT NOT NULL CHECK (author IN ('user', 'identification')),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    -- Only a release names a release; File Tags names the candidate's own files.
    CHECK ((kind = 'external_release') = (source IS NOT NULL)),
    CHECK ((kind = 'external_release') = (release_id IS NOT NULL)),
    -- Identification only ever concludes a release. Reading a folder's own tags
    -- is something a person asks for.
    CHECK (author != 'identification' OR kind = 'external_release')
) STRICT;

INSERT INTO import_candidate_draft_provenance (content_hash, kind, source, release_id, author)
SELECT content_hash, provenance_kind, provenance_source, provenance_release_id, provenance_author
FROM import_candidate_state_v3
WHERE provenance_kind IS NOT NULL;

-- Find online pairs a MusicBrainz release and a Discogs release into one
-- pressing row when they agree on a barcode or a catalog number. Picking that
-- row claims both, and the provenance can only name the one the draft is read
-- from. One row per other source the pick carries, hanging off the provenance.
CREATE TABLE import_candidate_provenance_partner_v2 (
    content_hash TEXT NOT NULL,
    source       TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id   TEXT NOT NULL,
    PRIMARY KEY (content_hash, source),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_draft_provenance (content_hash) ON DELETE CASCADE
) STRICT;

-- Only an external release can carry partners; a partner row under any other
-- provenance was never read back.
INSERT INTO import_candidate_provenance_partner_v2 (content_hash, source, release_id)
SELECT content_hash, source, release_id
FROM import_candidate_provenance_partner
WHERE content_hash IN (
    SELECT content_hash FROM import_candidate_draft_provenance WHERE kind = 'external_release'
);

DROP TABLE import_candidate_provenance_partner;
ALTER TABLE import_candidate_provenance_partner_v2
    RENAME TO import_candidate_provenance_partner;

-- The cover applied to this candidate, and the bytes prepared for a remote one.
CREATE TABLE import_candidate_cover_v2 (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('local', 'remote', 'embedded')),
    file_id      TEXT,
    url          TEXT,
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((kind IN ('local', 'embedded')) = (file_id IS NOT NULL)),
    CHECK ((kind = 'remote') = (url IS NOT NULL AND source IS NOT NULL))
) STRICT;

INSERT INTO import_candidate_cover_v2 (content_hash, kind, file_id, url, source)
SELECT content_hash, kind, file_id, url, source FROM import_candidate_cover;

CREATE TABLE import_candidate_remote_cover_asset_v2 (
    content_hash TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    bytes        BLOB NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_cover_v2 (content_hash) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_remote_cover_asset_v2 (content_hash, content_type, bytes)
SELECT content_hash, content_type, bytes FROM import_candidate_remote_cover_asset;

DROP TABLE import_candidate_remote_cover_asset;
ALTER TABLE import_candidate_remote_cover_asset_v2
    RENAME TO import_candidate_remote_cover_asset;

DROP TABLE import_candidate_cover;
ALTER TABLE import_candidate_cover_v2 RENAME TO import_candidate_cover;

-- The last import of this candidate that failed, so the pane still offers
-- Retry after a relaunch, and the artist identity conflict behind one.
CREATE TABLE import_candidate_failure_v2 (
    content_hash TEXT PRIMARY KEY,
    error        TEXT NOT NULL,
    failed_at    TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_failure_v2 (content_hash, error, failed_at)
SELECT content_hash, error, failed_at FROM import_candidate_failure;

CREATE TABLE import_candidate_artist_identity_conflict_v2 (
    content_hash                  TEXT PRIMARY KEY,
    incoming_artist_name          TEXT NOT NULL,
    discogs_artist_id             TEXT NOT NULL,
    musicbrainz_artist_id         TEXT NOT NULL,
    discogs_library_artist_id     TEXT NOT NULL,
    musicbrainz_library_artist_id TEXT NOT NULL,
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_failure_v2 (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (discogs_library_artist_id) REFERENCES artists (id) ON DELETE RESTRICT,
    FOREIGN KEY (musicbrainz_library_artist_id) REFERENCES artists (id) ON DELETE RESTRICT
) STRICT;

INSERT INTO import_candidate_artist_identity_conflict_v2 (
    content_hash, incoming_artist_name, discogs_artist_id, musicbrainz_artist_id,
    discogs_library_artist_id, musicbrainz_library_artist_id
)
SELECT content_hash, incoming_artist_name, discogs_artist_id, musicbrainz_artist_id,
       discogs_library_artist_id, musicbrainz_library_artist_id
FROM import_candidate_artist_identity_conflict;

DROP TABLE import_candidate_artist_identity_conflict;
ALTER TABLE import_candidate_artist_identity_conflict_v2
    RENAME TO import_candidate_artist_identity_conflict;

DROP TABLE import_candidate_failure;
ALTER TABLE import_candidate_failure_v2 RENAME TO import_candidate_failure;

-- The signals identification settled on, and the values of their three lists.
CREATE TABLE import_candidate_signals_v2 (
    content_hash           TEXT PRIMARY KEY,
    disc_id_state          TEXT NOT NULL CHECK (disc_id_state IN ('computed', 'absent', 'failed')),
    disc_id                TEXT,
    -- The candidate-relative path of the LOG or CUE the disc ID came from, so a
    -- surface can put it on that file's row. NULL for a re-identify pass over a
    -- library release, which derives the ID from stored tracks rather than a
    -- file of a scanned folder.
    disc_id_source_file    TEXT,
    track_count            INTEGER NOT NULL CHECK (track_count >= 0),
    disc_id_failure        TEXT CHECK (disc_id_failure IS NULL OR disc_id_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    disc_id_failure_status INTEGER,
    disc_id_failure_detail TEXT,
    barcode_state          TEXT NOT NULL CHECK (barcode_state IN ('settled', 'failed', 'absent')),
    barcode_failure        TEXT CHECK (barcode_failure IS NULL OR barcode_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    barcode_failure_status INTEGER,
    barcode_failure_detail TEXT,
    text_state             TEXT NOT NULL CHECK (text_state IN ('settled', 'failed')),
    text_failure           TEXT CHECK (text_failure IS NULL OR text_failure IN ('network', 'provider', 'timeout', 'artwork_analysis', 'diagnostic')),
    text_failure_status    INTEGER,
    text_failure_detail    TEXT,
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

INSERT INTO import_candidate_signals_v2 (
    content_hash, disc_id_state, disc_id, disc_id_source_file, track_count,
    disc_id_failure, disc_id_failure_status, disc_id_failure_detail,
    barcode_state, barcode_failure, barcode_failure_status, barcode_failure_detail,
    text_state, text_failure, text_failure_status, text_failure_detail
)
SELECT content_hash, disc_id_state, disc_id, disc_id_source_file, track_count,
       disc_id_failure, disc_id_failure_status, disc_id_failure_detail,
       barcode_state, barcode_failure, barcode_failure_status, barcode_failure_detail,
       text_state, text_failure, text_failure_status, text_failure_detail
FROM import_candidate_signals;

CREATE TABLE import_candidate_signal_value_v2 (
    content_hash TEXT NOT NULL,
    list         TEXT NOT NULL CHECK (list IN ('barcode', 'catalog', 'free_text')),
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT NOT NULL,
    origin       TEXT CHECK (origin IS NULL OR origin IN ('disc_toc', 'cue_sheet', 'artwork', 'folder_name', 'filename', 'text_file')),
    -- The candidate-relative path of the file the value was read off, where the
    -- origin is a file: the image OCR found a barcode on, the sheet a field came
    -- from. NULL where the origin names no file (the folder's own name), and for
    -- a re-identify pass over a library release, whose images are stored blobs.
    origin_path  TEXT,
    PRIMARY KEY (content_hash, list, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_signals_v2 (content_hash) ON DELETE CASCADE,
    CHECK ((list = 'free_text') = (origin IS NULL)),
    -- A file with no origin behind it is not a provenance.
    CHECK (origin_path IS NULL OR origin IS NOT NULL)
) STRICT;

INSERT INTO import_candidate_signal_value_v2 (content_hash, list, position, value, origin, origin_path)
SELECT content_hash, list, position, value, origin, origin_path FROM import_candidate_signal_value;

DROP TABLE import_candidate_signal_value;
ALTER TABLE import_candidate_signal_value_v2 RENAME TO import_candidate_signal_value;

DROP TABLE import_candidate_signals;
ALTER TABLE import_candidate_signals_v2 RENAME TO import_candidate_signals;

-- One file's user decisions: its role, which audio a sheet describes, which
-- disc a sheet is.
CREATE TABLE import_candidate_file_edit_v2 (
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

INSERT INTO import_candidate_file_edit_v2 (
    content_hash, relative_path, role_choice, sheet_binding, sheet_binding_file_id,
    sheet_disc, sheet_disc_number
)
SELECT content_hash, relative_path, role_choice, sheet_binding, sheet_binding_file_id,
       sheet_disc, sheet_disc_number
FROM import_candidate_file_edit;

DROP TABLE import_candidate_file_edit;
ALTER TABLE import_candidate_file_edit_v2 RENAME TO import_candidate_file_edit;

-- What the pane was showing for this candidate when it was last open.
CREATE TABLE import_candidate_session_v2 (
    content_hash   TEXT PRIMARY KEY,
    presentation   TEXT NOT NULL CHECK (presentation IN ('draft', 'find_online', 'file_tags')),
    search_tab     TEXT NOT NULL CHECK (search_tab IN ('general', 'catalog_number', 'barcode')),
    search_artist  TEXT NOT NULL,
    search_album   TEXT NOT NULL,
    search_catalog TEXT NOT NULL,
    search_barcode TEXT NOT NULL,
    error          TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_session_v2 (
    content_hash, presentation, search_tab, search_artist, search_album,
    search_catalog, search_barcode, error
)
SELECT content_hash, presentation, search_tab, search_artist, search_album,
       search_catalog, search_barcode, error
FROM import_candidate_session;

DROP TABLE import_candidate_session;
ALTER TABLE import_candidate_session_v2 RENAME TO import_candidate_session;

-- Presence means the candidate's current metadata revision has a complete
-- provider answer set, including the legitimate empty set.
CREATE TABLE import_candidate_asset_preparation_v2 (
    content_hash TEXT PRIMARY KEY,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_asset_preparation_v2 (content_hash)
SELECT content_hash FROM import_candidate_asset_preparation;

DROP TABLE import_candidate_asset_preparation;
ALTER TABLE import_candidate_asset_preparation_v2 RENAME TO import_candidate_asset_preparation;

CREATE TABLE import_candidate_artist_asset_v2 (
    content_hash      TEXT NOT NULL,
    discogs_artist_id TEXT NOT NULL,
    answer            TEXT NOT NULL CHECK (answer IN ('image', 'nothing')),
    source_url        TEXT,
    content_type      TEXT,
    bytes             BLOB,
    PRIMARY KEY (content_hash, discogs_artist_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK (
        (answer = 'image' AND source_url IS NOT NULL AND content_type IS NOT NULL AND bytes IS NOT NULL)
        OR
        (answer = 'nothing' AND source_url IS NULL AND content_type IS NULL AND bytes IS NULL)
    )
) STRICT;

INSERT INTO import_candidate_artist_asset_v2 (
    content_hash, discogs_artist_id, answer, source_url, content_type, bytes
)
SELECT content_hash, discogs_artist_id, answer, source_url, content_type, bytes
FROM import_candidate_artist_asset;

DROP TABLE import_candidate_artist_asset;
ALTER TABLE import_candidate_artist_asset_v2 RENAME TO import_candidate_artist_asset;

CREATE TABLE import_candidate_source_artist_v2 (
    content_hash      TEXT NOT NULL,
    discogs_artist_id TEXT NOT NULL,
    PRIMARY KEY (content_hash, discogs_artist_id),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_source_artist_v2 (content_hash, discogs_artist_id)
SELECT content_hash, discogs_artist_id FROM import_candidate_source_artist;

DROP TABLE import_candidate_source_artist;
ALTER TABLE import_candidate_source_artist_v2 RENAME TO import_candidate_source_artist;

-- Which watched roots list a candidate, so removing one forgets only the
-- candidates no other root still lists.
CREATE TABLE import_candidate_watched_root_v2 (
    content_hash        TEXT NOT NULL,
    watched_folder_path TEXT NOT NULL,
    PRIMARY KEY (content_hash, watched_folder_path),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_watched_root_v2 (content_hash, watched_folder_path)
SELECT content_hash, watched_folder_path FROM import_candidate_watched_root;

DROP TABLE import_candidate_watched_root;
ALTER TABLE import_candidate_watched_root_v2 RENAME TO import_candidate_watched_root;

CREATE INDEX import_candidate_watched_root_by_root
    ON import_candidate_watched_root (watched_folder_path);

DROP TABLE import_candidate_state_v3;
