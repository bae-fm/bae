-- What the scans store of each watched folder: the folders they found and
-- when, each root's last scan, the releases it read and their files, tags,
-- parts, sidecars and cue sheets, and what it skipped. Every path on disk in
-- these tables is coven's bytes for it (`coven::path_bytes`), so a folder
-- whose name isn't valid Unicode keys its own rows; `coven_path_text` shows
-- one.

-- When the scans first saw each folder, the date the folder carries, and
-- whether a scan has read it as a release or as broken: a valid release read
-- where it has not is newly found. Only scans write it: a release built from
-- folders picked together is dated by theirs.
CREATE TABLE IF NOT EXISTS folder_discovery (
    folder           BLOB PRIMARY KEY,
    first_seen_at    INTEGER NOT NULL,
    settled          INTEGER NOT NULL CHECK (settled IN (0, 1)),
    source_date      INTEGER,
    source_date_kind TEXT CHECK ((source_date IS NULL AND source_date_kind IS NULL)
        OR (source_date IS NOT NULL AND source_date_kind IS NOT NULL
            AND source_date_kind IN ('added_to_directory', 'created')))
) STRICT;

-- The one row handing out scan generations, so each root's generation is
-- stored before its scan begins.
CREATE TABLE IF NOT EXISTS folder_scan_generation_sequence (
    singleton       INTEGER PRIMARY KEY CHECK (singleton = 1),
    last_generation INTEGER NOT NULL CHECK (last_generation >= 0)
) STRICT;

INSERT OR IGNORE INTO folder_scan_generation_sequence (singleton, last_generation)
VALUES (1, 0);

-- This device's last scan of each watched folder. A scan writes entries as it
-- finds them; completing removes the ones it did not see, in the transaction
-- that marks it complete. A failed or interrupted scan keeps old and new ones.
-- `status` is how the current generation's scan ended, NULL until it stores
-- that; whether a scan is reading the folder is never stored, since nothing
-- stored could stop saying so when the process reading it ends.
CREATE TABLE IF NOT EXISTS folder_scan_roots (
    watched_folder_path BLOB PRIMARY KEY,
    generation          INTEGER NOT NULL CHECK (generation >= 0),
    status              TEXT CHECK (status IN ('complete', 'failed')),
    error               TEXT,
    -- The volume the folder was on when the scan began, asked once per scan so
    -- reading scan state never waits on a mount.
    volume              TEXT NOT NULL CHECK (volume IN ('local', 'network')),
    -- How many folder candidates the current generation holds, kept by the
    -- triggers below so reading the scan's progress never counts them.
    found_count         INTEGER NOT NULL DEFAULT 0 CHECK (found_count >= 0),
    CHECK ((status IS 'failed') = (error IS NOT NULL)),
    FOREIGN KEY (watched_folder_path)
        REFERENCES watched_import_folders (path)
        ON DELETE CASCADE
) STRICT;

-- The directories seen under a watched folder and when each last changed, so a
-- rescan can skip the ones that did not.
CREATE TABLE IF NOT EXISTS folder_scan_directory (
    watched_folder_path BLOB NOT NULL,
    path                BLOB NOT NULL,
    modified_at         INTEGER NOT NULL,
    PRIMARY KEY (watched_folder_path, path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE
) STRICT;

-- One release the scan found: a folder, or folders a grouping reads as one.
-- `path` is the release's key (`CandidateKey`) — its folder's path, or its
-- grouping's key.
CREATE TABLE IF NOT EXISTS scan_candidate (
    watched_folder_path            BLOB NOT NULL,
    path                           BLOB NOT NULL,
    generation                     INTEGER NOT NULL CHECK (generation >= 0),
    kind                           TEXT NOT NULL CHECK (kind IN ('tentative', 'valid', 'invalid')),
    name                           TEXT NOT NULL,
    -- The folder the release is shown as below its watched folder, as
    -- `RelativeFolder` bytes; `coven_path_text` shows it.
    relative_folder                BLOB NOT NULL,
    -- The folder the release is shown as.
    folder                         BLOB NOT NULL,
    -- The folder its files are read from, and whether all of them below it.
    file_root                      BLOB NOT NULL,
    scope                          TEXT NOT NULL CHECK (scope IN ('direct', 'recursive')),
    content_hash                   TEXT,
    file_edit_revision             INTEGER NOT NULL DEFAULT 0 CHECK (file_edit_revision >= 0),
    grouping_key                   BLOB CHECK (grouping_key IS NULL OR grouping_key = path),
    -- 'grouping' for a release a grouping with no anchor builds from its
    -- members.
    source_kind                    TEXT NOT NULL DEFAULT 'folder' CHECK (source_kind IN ('folder', 'grouping')),
    PRIMARY KEY (watched_folder_path, path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE,
    -- A release a grouping reads as one leaves the queue with its grouping.
    -- Checked as the write commits, since a write stores the two in either
    -- order.
    FOREIGN KEY (grouping_key) REFERENCES release_grouping (key)
        ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
    CHECK ((kind = 'invalid') = (content_hash IS NULL)),
    CHECK ((source_kind = 'grouping') <= (grouping_key IS NOT NULL))
) STRICT;

CREATE INDEX IF NOT EXISTS idx_scan_candidate_path ON scan_candidate (path);

-- `folder_scan_roots.found_count`: the folder candidates of the root's
-- current generation. A candidate counts while it is a folder's and carries
-- its root's generation; each trigger moves the count by what its write
-- changed about that, and a root's new generation is counted once as it
-- begins.
CREATE TRIGGER IF NOT EXISTS scan_candidate_found_insert
AFTER INSERT ON scan_candidate
WHEN NEW.source_kind = 'folder'
BEGIN
    UPDATE folder_scan_roots SET found_count = found_count + 1
    WHERE watched_folder_path = NEW.watched_folder_path AND generation = NEW.generation;
END;

CREATE TRIGGER IF NOT EXISTS scan_candidate_found_delete
AFTER DELETE ON scan_candidate
WHEN OLD.source_kind = 'folder'
BEGIN
    UPDATE folder_scan_roots SET found_count = found_count - 1
    WHERE watched_folder_path = OLD.watched_folder_path AND generation = OLD.generation;
END;

CREATE TRIGGER IF NOT EXISTS scan_candidate_found_update
AFTER UPDATE OF generation, source_kind ON scan_candidate
BEGIN
    UPDATE folder_scan_roots SET found_count = found_count - 1
    WHERE OLD.source_kind = 'folder'
      AND watched_folder_path = OLD.watched_folder_path AND generation = OLD.generation;
    UPDATE folder_scan_roots SET found_count = found_count + 1
    WHERE NEW.source_kind = 'folder'
      AND watched_folder_path = NEW.watched_folder_path AND generation = NEW.generation;
END;

CREATE TRIGGER IF NOT EXISTS folder_scan_roots_found_generation
AFTER UPDATE OF generation ON folder_scan_roots
BEGIN
    UPDATE folder_scan_roots SET found_count = (
        SELECT COUNT(*) FROM scan_candidate AS candidate
        WHERE candidate.watched_folder_path = NEW.watched_folder_path
          AND candidate.generation = NEW.generation
          AND candidate.source_kind = 'folder')
    WHERE watched_folder_path = NEW.watched_folder_path;
END;

-- The release a grouping reads as one, which goes with the grouping.
CREATE INDEX IF NOT EXISTS idx_scan_candidate_grouping ON scan_candidate (grouping_key);

CREATE INDEX IF NOT EXISTS idx_scan_candidate_content_hash
    ON scan_candidate (content_hash) WHERE content_hash IS NOT NULL;

-- The entries under a watched folder reading a folder's files, which a scan
-- write looks up by the root and the folder, the folders above it and the
-- range below it.
CREATE INDEX IF NOT EXISTS idx_scan_candidate_file_root
    ON scan_candidate (watched_folder_path, file_root);

-- Every file of a candidate folder, the role the scan read it as, and the audio
-- facts probing found in it. `relative_path` is the file's name in the release,
-- each component as `coven::path_text` shows it; `absolute_path` is coven's
-- bytes for its path (`coven::path_bytes`), which open it.
CREATE TABLE IF NOT EXISTS scan_candidate_file (
    watched_folder_path   BLOB NOT NULL,
    candidate_path        BLOB NOT NULL,
    relative_path         TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    absolute_path         BLOB NOT NULL,
    size                  INTEGER NOT NULL CHECK (size >= 0),
    modified_at_ns        INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    audio_content_type    TEXT,
    audio_container       TEXT,
    audio_duration_ms     INTEGER CHECK (audio_duration_ms IS NULL OR audio_duration_ms >= 0),
    audio_sample_rate_hz  INTEGER CHECK (audio_sample_rate_hz IS NULL OR audio_sample_rate_hz > 0),
    audio_bits_per_sample INTEGER CHECK (audio_bits_per_sample IS NULL OR audio_bits_per_sample > 0),
    audio_bitrate_kbps    INTEGER CHECK (audio_bitrate_kbps IS NULL OR audio_bitrate_kbps > 0),
    audio_channels        INTEGER CHECK (audio_channels IS NULL OR audio_channels > 0),
    file_name             TEXT NOT NULL,
    dir_prefix            TEXT,
    role                  TEXT NOT NULL CHECK (role IN ('audio', 'track_sheet', 'artwork', 'damaged_image', 'document', 'other')),
    -- Which document the file is, as its name reads; set for a document, and
    -- only a document.
    document_kind         TEXT CHECK (document_kind IS NULL OR document_kind IN ('cue_sheet', 'rip_log', 'accuraterip_report', 'playlist', 'text')),
    sheet_binding         TEXT CHECK (sheet_binding IS NULL OR sheet_binding IN ('resolved', 'unresolved', 'refused_timing', 'refused_codec')),
    sheet_binding_codec   TEXT,
    sheet_disc            TEXT CHECK (sheet_disc IS NULL OR sheet_disc IN ('disc', 'ignored')),
    sheet_disc_number     INTEGER CHECK (sheet_disc_number IS NULL OR sheet_disc_number >= 1),
    PRIMARY KEY (watched_folder_path, candidate_path, relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path) REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE,
    CHECK ((role = 'track_sheet') = (sheet_binding IS NOT NULL AND sheet_disc IS NOT NULL)),
    CHECK ((role = 'document') = (document_kind IS NOT NULL)),
    CHECK ((sheet_binding = 'refused_codec') = (sheet_binding_codec IS NOT NULL)),
    CHECK ((sheet_disc = 'disc') = (sheet_disc_number IS NOT NULL)),
    CHECK (
        (role <> 'audio' AND audio_content_type IS NULL AND audio_container IS NULL
            AND audio_duration_ms IS NULL
            AND audio_sample_rate_hz IS NULL AND audio_bits_per_sample IS NULL
            AND audio_bitrate_kbps IS NULL AND audio_channels IS NULL)
        OR
        (role = 'audio' AND audio_content_type IS NOT NULL AND audio_container IS NOT NULL
            AND audio_duration_ms IS NOT NULL
            AND audio_sample_rate_hz IS NOT NULL AND audio_channels IS NOT NULL
            AND (
                (audio_content_type IN (
                    'audio/flac', 'audio/x-ape', 'audio/alac', 'audio/pcm',
                    'audio/wavpack', 'audio/dsd'
                ) AND audio_bits_per_sample IS NOT NULL AND audio_bitrate_kbps IS NULL)
                OR
                (audio_content_type IN (
                    'audio/mpeg', 'audio/aac', 'audio/opus', 'audio/vorbis'
                ) AND audio_bits_per_sample IS NULL AND audio_bitrate_kbps IS NOT NULL)
            ))
    )
) STRICT;

-- Which release holds a file on disk: what a sidecar holding the same file
-- replaces.
CREATE INDEX IF NOT EXISTS idx_scan_candidate_file_absolute
    ON scan_candidate_file (absolute_path);

-- What a candidate's files were probed at, and the cover its tags carried.
CREATE TABLE IF NOT EXISTS scan_candidate_tag_snapshot (
    watched_folder_path                 BLOB NOT NULL,
    candidate_path                      BLOB NOT NULL,
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

-- The tags each audio file of a candidate carried when it was probed: what
-- they state, or, for tags that do not parse, why, with every column of what
-- they state empty.
CREATE TABLE IF NOT EXISTS scan_candidate_file_tag (
    watched_folder_path BLOB NOT NULL,
    candidate_path      BLOB NOT NULL,
    relative_path       TEXT NOT NULL,
    file_size           INTEGER NOT NULL CHECK (file_size >= 0),
    modified_at_ns      INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    -- Why the tags do not parse; NULL for tags that do.
    unparseable_reason  TEXT CHECK (unparseable_reason <> ''),
    title               TEXT,
    -- Every value a multi-valued tag states, in order, as a JSON array of
    -- strings: the track's artists and the album's artists.
    track_artists       TEXT
        CHECK (json_valid(track_artists) AND json_type(track_artists) = 'array'),
    album_title         TEXT,
    album_artists       TEXT
        CHECK (json_valid(album_artists) AND json_type(album_artists) = 'array'),
    -- Whether the tags flag the file part of a compilation.
    compilation         INTEGER CHECK (compilation IN (0, 1)),
    -- Every year the file's dates name, each once, as a JSON array of integers.
    years               TEXT
        CHECK (json_valid(years) AND json_type(years) = 'array'),
    track_number        INTEGER,
    disc_number         INTEGER,
    CHECK (
        (unparseable_reason IS NULL
            AND track_artists IS NOT NULL AND album_artists IS NOT NULL
            AND compilation IS NOT NULL AND years IS NOT NULL)
        OR
        (unparseable_reason IS NOT NULL
            AND title IS NULL AND track_artists IS NULL AND album_title IS NULL
            AND album_artists IS NULL AND compilation IS NULL AND years IS NULL
            AND track_number IS NULL AND disc_number IS NULL)
    ),
    PRIMARY KEY (watched_folder_path, candidate_path, relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate_tag_snapshot (watched_folder_path, candidate_path) ON DELETE CASCADE,
    FOREIGN KEY (watched_folder_path, candidate_path, relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE
) STRICT;

-- The folders a release read from several is made of, in play order, and the
-- prefix each one's files take in the release.
CREATE TABLE IF NOT EXISTS scan_candidate_part (
    watched_folder_path BLOB NOT NULL,
    candidate_path      BLOB NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    folder              BLOB NOT NULL,
    prefix              TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, position),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE
) STRICT;

-- Every file of a release that failed validation, in name order, as the scan
-- read it: by its name, and for audio and images by its bytes. A name two of
-- its files show, else its damaged audio, is why it failed, and is the reason
-- the Skipped tab gives. Names and paths are as in `scan_candidate_file`; two
-- files can show one name, and only the path is one file's.
CREATE TABLE IF NOT EXISTS scan_skipped_file (
    watched_folder_path BLOB NOT NULL,
    candidate_path      BLOB NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    relative_path       TEXT NOT NULL,
    absolute_path       BLOB NOT NULL,
    size                INTEGER NOT NULL CHECK (size >= 0),
    modified_at_ns      INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    role                TEXT NOT NULL CHECK (role IN ('audio', 'damaged_audio', 'track_sheet', 'artwork', 'damaged_image', 'document', 'other')),
    -- Why a damaged track can't be played: it holds no bytes, or no stream bae plays.
    audio_damage        TEXT CHECK (audio_damage IS NULL OR audio_damage IN ('empty', 'unreadable')),
    -- Which document the file is, as in `scan_candidate_file`.
    document_kind       TEXT CHECK (document_kind IS NULL OR document_kind IN ('cue_sheet', 'rip_log', 'accuraterip_report', 'playlist', 'text')),
    PRIMARY KEY (watched_folder_path, candidate_path, position),
    UNIQUE (watched_folder_path, candidate_path, absolute_path),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE,
    CHECK ((role = 'damaged_audio') = (audio_damage IS NOT NULL)),
    CHECK ((role = 'document') = (document_kind IS NOT NULL))
) STRICT;

-- What the tags of a release that failed validation call it: the album title
-- the first of its playable files to state one gives, with that file's
-- artist. No row when none states one, and for a grouping's release.
CREATE TABLE IF NOT EXISTS scan_skipped_tags (
    watched_folder_path BLOB NOT NULL,
    candidate_path      BLOB NOT NULL,
    album_title         TEXT NOT NULL,
    artist              TEXT,
    PRIMARY KEY (watched_folder_path, candidate_path),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE
) STRICT;

-- The files under a folder that no release the scan read there owns, such as
-- a cover or booklet beside disc folders kept as separate releases. Stored and
-- pruned like the scan's candidates. A sidecar and a scanned release holding
-- the same file replace each other, whichever is written later. Watched roots
-- never overlap, so the folder alone is the key.
CREATE TABLE IF NOT EXISTS scan_sidecar (
    folder              BLOB PRIMARY KEY,
    watched_folder_path BLOB NOT NULL,
    generation          INTEGER NOT NULL CHECK (generation >= 0),
    -- 'invalid' when a zero-byte track is among its files or two of them show
    -- one name, so a release taking them in cannot be imported; 'downloading'
    -- while a download into the folder runs, when none of its files is stored.
    state               TEXT NOT NULL CHECK (state IN ('valid', 'invalid', 'downloading')),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_scan_sidecar_root ON scan_sidecar (watched_folder_path);

-- One file of a sidecar, in release file order, with its role. Only an
-- invalid sidecar holds a damaged track, or two files showing one name. Names
-- and paths are as in `scan_candidate_file`.
CREATE TABLE IF NOT EXISTS scan_sidecar_file (
    folder              BLOB NOT NULL REFERENCES scan_sidecar (folder) ON DELETE CASCADE,
    position            INTEGER NOT NULL CHECK (position >= 0),
    relative_path       TEXT NOT NULL,
    absolute_path       BLOB NOT NULL UNIQUE,
    size                INTEGER NOT NULL CHECK (size >= 0),
    modified_at_ns      INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    role                TEXT NOT NULL CHECK (role IN ('artwork', 'damaged_image', 'document', 'other', 'damaged_audio')),
    audio_damage        TEXT CHECK (audio_damage IS NULL OR audio_damage IN ('empty', 'unreadable')),
    -- Which document the file is, as in `scan_candidate_file`.
    document_kind       TEXT CHECK (document_kind IS NULL OR document_kind IN ('cue_sheet', 'rip_log', 'accuraterip_report', 'playlist', 'text')),
    PRIMARY KEY (folder, position),
    CHECK ((role = 'damaged_audio') = (audio_damage IS NOT NULL)),
    CHECK ((role = 'document') = (document_kind IS NOT NULL))
) STRICT;

-- A CUE sheet found beside a candidate's audio, as parsed.
CREATE TABLE IF NOT EXISTS scan_cue_sheet (
    watched_folder_path BLOB NOT NULL,
    candidate_path      BLOB NOT NULL,
    sheet_relative_path TEXT NOT NULL,
    title               TEXT,
    performer           TEXT,
    catalog             TEXT,
    date                TEXT,
    -- The CD ripper that wrote the sheet, where its REM COMMENT names one.
    ripper              TEXT CHECK (ripper IS NULL OR ripper IN ('exact_audio_copy')),
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE
) STRICT;

-- One track of a CUE sheet, with the span it declares and the silence a PREGAP
-- directive puts before it. An audio pregap is the track's INDEX 00.
CREATE TABLE IF NOT EXISTS scan_cue_track (
    watched_folder_path         BLOB NOT NULL,
    candidate_path              BLOB NOT NULL,
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
    generated_pregap_frames     INTEGER CHECK (generated_pregap_frames IS NULL OR generated_pregap_frames >= 0),
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path, position),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_cue_sheet (watched_folder_path, candidate_path, sheet_relative_path) ON DELETE CASCADE,
    CHECK ((mode = 'other') = (mode_other IS NOT NULL))
) STRICT;

-- Every INDEX line of a CUE track, in the order the sheet stated them.
CREATE TABLE IF NOT EXISTS scan_cue_index (
    watched_folder_path BLOB NOT NULL,
    candidate_path      BLOB NOT NULL,
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

-- Which audio file each FILE reference of a CUE sheet resolved to.
CREATE TABLE IF NOT EXISTS scan_sheet_audio_file (
    watched_folder_path BLOB NOT NULL,
    candidate_path      BLOB NOT NULL,
    sheet_relative_path TEXT NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    file_reference      TEXT NOT NULL,
    audio_relative_path TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, sheet_relative_path, position),
    UNIQUE (watched_folder_path, candidate_path, sheet_relative_path, file_reference),
    UNIQUE (watched_folder_path, candidate_path, sheet_relative_path, audio_relative_path),
    FOREIGN KEY (watched_folder_path, candidate_path, sheet_relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE,
    FOREIGN KEY (watched_folder_path, candidate_path, audio_relative_path)
        REFERENCES scan_candidate_file (watched_folder_path, candidate_path, relative_path) ON DELETE CASCADE
) STRICT;

-- What the import list's triggers look a changed row's releases up by.
CREATE INDEX IF NOT EXISTS idx_scan_candidate_folder ON scan_candidate (folder);
