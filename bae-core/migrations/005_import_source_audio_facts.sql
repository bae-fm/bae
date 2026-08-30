CREATE TABLE migration_004_empty_scan_guard (
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;

INSERT INTO migration_004_empty_scan_guard (valid)
SELECT CASE WHEN COUNT(*) = 0 THEN 1 ELSE 0 END
FROM folder_scan_roots;

DROP TABLE migration_004_empty_scan_guard;
DROP TABLE scan_candidate_file_tag;
DROP TABLE scan_candidate_tag_snapshot;
DROP TABLE scan_cue_index;
DROP TABLE scan_cue_track;
DROP TABLE scan_cue_sheet;
DROP TABLE scan_candidate_resolved_boundary;
DROP TABLE scan_candidate_file;
DROP TABLE scan_candidate;
DROP TABLE import_candidate_file_duration;

CREATE TABLE scan_candidate (
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
    initial_metadata_source        TEXT CHECK (initial_metadata_source IS NULL OR initial_metadata_source IN ('find_online', 'file_tags', 'none')),
    combine_ancestor_relative_path TEXT,
    invalid_reason                 TEXT CHECK (invalid_reason IS NULL OR invalid_reason IN ('corrupt_audio', 'corrupt_image', 'no_valid_audio')),
    invalid_reason_path            TEXT,
    PRIMARY KEY (watched_folder_path, path),
    FOREIGN KEY (watched_folder_path) REFERENCES folder_scan_roots (watched_folder_path) ON DELETE CASCADE,
    CHECK ((kind = 'invalid') = (invalid_reason IS NOT NULL)),
    CHECK ((kind = 'invalid') = (file_root IS NULL AND scope IS NULL AND content_hash IS NULL AND initial_metadata_source IS NULL)),
    CHECK ((invalid_reason IN ('corrupt_audio', 'corrupt_image')) = (invalid_reason_path IS NOT NULL))
) STRICT;

CREATE TABLE scan_candidate_file (
    watched_folder_path   TEXT NOT NULL,
    candidate_path        TEXT NOT NULL,
    relative_path         TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    absolute_path         TEXT NOT NULL,
    size                  INTEGER NOT NULL CHECK (size >= 0),
    modified_at_ns        INTEGER NOT NULL CHECK (modified_at_ns >= 0),
    content_digest        TEXT NOT NULL CHECK (length(content_digest) = 64),
    audio_content_type    TEXT,
    audio_duration_ms     INTEGER CHECK (audio_duration_ms IS NULL OR audio_duration_ms >= 0),
    audio_sample_rate_hz  INTEGER CHECK (audio_sample_rate_hz IS NULL OR audio_sample_rate_hz > 0),
    audio_bits_per_sample INTEGER CHECK (audio_bits_per_sample IS NULL OR audio_bits_per_sample > 0),
    audio_bitrate_kbps    INTEGER CHECK (audio_bitrate_kbps IS NULL OR audio_bitrate_kbps > 0),
    audio_channels        INTEGER CHECK (audio_channels IS NULL OR audio_channels > 0),
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
    CHECK ((sheet_disc = 'disc') = (sheet_disc_number IS NOT NULL)),
    CHECK (
        (proposed_audio = 0 AND audio_content_type IS NULL AND audio_duration_ms IS NULL
            AND audio_sample_rate_hz IS NULL AND audio_bits_per_sample IS NULL
            AND audio_bitrate_kbps IS NULL AND audio_channels IS NULL)
        OR
        (proposed_audio = 1 AND audio_content_type IS NOT NULL AND audio_duration_ms IS NOT NULL
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

CREATE TABLE scan_cue_sheet (
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

CREATE TABLE scan_cue_track (
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

CREATE TABLE scan_cue_index (
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

CREATE TABLE scan_candidate_resolved_boundary (
    watched_folder_path  TEXT NOT NULL,
    candidate_path       TEXT NOT NULL,
    position             INTEGER NOT NULL CHECK (position >= 0),
    relative_folder_path TEXT NOT NULL,
    decision             TEXT NOT NULL CHECK (decision IN ('combine_as_one_release', 'keep_as_separate_releases')),
    name                 TEXT NOT NULL,
    display_path         TEXT NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, position),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE
) STRICT;

ALTER TABLE release_files ADD COLUMN source_audio_layout TEXT
    CHECK (source_audio_layout IS NULL OR source_audio_layout IN ('file', 'cue'));
ALTER TABLE release_files ADD COLUMN source_audio_content_type TEXT;
ALTER TABLE release_files ADD COLUMN source_audio_duration_ms INTEGER
    CHECK (source_audio_duration_ms IS NULL OR source_audio_duration_ms >= 0);
ALTER TABLE release_files ADD COLUMN source_audio_sample_rate_hz INTEGER
    CHECK (source_audio_sample_rate_hz IS NULL OR source_audio_sample_rate_hz > 0);
ALTER TABLE release_files ADD COLUMN source_audio_bits_per_sample INTEGER
    CHECK (source_audio_bits_per_sample IS NULL OR source_audio_bits_per_sample > 0);
ALTER TABLE release_files ADD COLUMN source_audio_bitrate_kbps INTEGER
    CHECK (source_audio_bitrate_kbps IS NULL OR source_audio_bitrate_kbps > 0);
ALTER TABLE release_files ADD COLUMN source_audio_channels INTEGER
    CHECK (
        (source_audio_layout IS NULL
            AND source_audio_content_type IS NULL
            AND source_audio_duration_ms IS NULL
            AND source_audio_sample_rate_hz IS NULL
            AND source_audio_bits_per_sample IS NULL
            AND source_audio_bitrate_kbps IS NULL
            AND source_audio_channels IS NULL)
        OR (
            source_audio_channels IS NOT NULL
            AND source_audio_channels > 0
            AND source_audio_content_type IS NOT NULL
            AND source_audio_duration_ms IS NOT NULL
            AND source_audio_sample_rate_hz IS NOT NULL
            AND (
                (source_audio_content_type IN (
                    'audio/flac', 'audio/x-ape', 'audio/alac', 'audio/pcm',
                    'audio/wavpack', 'audio/dsd'
                ) AND source_audio_bits_per_sample IS NOT NULL
                    AND source_audio_bitrate_kbps IS NULL)
                OR
                (source_audio_content_type IN (
                    'audio/mpeg', 'audio/aac', 'audio/opus', 'audio/vorbis'
                ) AND source_audio_bits_per_sample IS NULL
                    AND source_audio_bitrate_kbps IS NOT NULL)
            )
        )
    );

CREATE INDEX idx_scan_candidate_path ON scan_candidate (path);
CREATE INDEX idx_scan_candidate_content_hash
    ON scan_candidate (content_hash) WHERE content_hash IS NOT NULL;
