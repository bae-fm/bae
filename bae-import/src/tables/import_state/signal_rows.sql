-- What extraction read off a candidate: what its files say about the medium
-- it was ripped from, its disc ID, and whether the barcode and text passes
-- settled or failed. Owned by the candidate's files (`content_hash`), as its
-- verdict was reached on, or by an identify run in flight (`identify_run`),
-- as the run last read it, its passes possibly still scanning.
CREATE TABLE IF NOT EXISTS import_candidate_signals (
    content_hash           TEXT UNIQUE,
    identify_run           INTEGER UNIQUE,
    -- Where a file proves the audio came from: a CD rip or a download, or NULL
    -- when nothing proves either.
    audio_source           TEXT CHECK (audio_source IS NULL OR audio_source IN ('cd_rip', 'download')),
    cd_rip_proof           TEXT CHECK (cd_rip_proof IS NULL OR cd_rip_proof IN ('rip_log', 'accurate_rip_report', 'ripper_sheet')),
    download_proof         TEXT CHECK (download_proof IS NULL OR download_proof IN ('itunes_purchase', 'bandcamp', 'delivery_set')),
    -- The candidate-relative path of the file that proves the source: the rip
    -- file, or the track a store marked. NULL for a label's delivery set, and
    -- when re-identifying a library release.
    audio_source_file      TEXT,
    -- The lossless rate, off a CD's, that rules a CD out.
    not_cd_rate            INTEGER CHECK (not_cd_rate IS NULL OR not_cd_rate > 0),
    -- A computed state's disc IDs are its `disc_id` signal values, one per
    -- disc, at least one.
    disc_id_state          TEXT NOT NULL CHECK (disc_id_state IN ('computed', 'absent', 'not_cd_audio', 'failed')),
    -- How bae broke reading each signal off the folder's files, as the error
    -- chain the person is shown.
    disc_id_failure        TEXT CHECK (disc_id_failure IS NULL OR disc_id_failure <> ''),
    barcode_state          TEXT NOT NULL CHECK (barcode_state IN ('scanning', 'settled', 'failed', 'absent')),
    barcode_failure        TEXT CHECK (barcode_failure IS NULL OR barcode_failure <> ''),
    text_state             TEXT NOT NULL CHECK (text_state IN ('scanning', 'settled', 'failed')),
    text_failure           TEXT CHECK (text_failure IS NULL OR text_failure <> ''),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (identify_run) REFERENCES import_identify_run (identify_run) ON DELETE CASCADE,
    CHECK ((content_hash IS NULL) <> (identify_run IS NULL)),
    -- Only a run in flight reads a pass still scanning.
    CHECK (identify_run IS NOT NULL OR (barcode_state <> 'scanning' AND text_state <> 'scanning')),
    CHECK ((audio_source IS 'cd_rip') = (cd_rip_proof IS NOT NULL)),
    CHECK ((audio_source IS 'download') = (download_proof IS NOT NULL)),
    CHECK (download_proof IS NOT 'delivery_set' OR audio_source_file IS NULL),
    CHECK (download_proof NOT IN ('itunes_purchase', 'bandcamp') OR audio_source_file IS NOT NULL),
    CHECK (audio_source IS NOT NULL OR audio_source_file IS NULL),
    -- A sheet goes unhashed only when the audio rules a CD out and nothing
    -- proves a CD rip.
    CHECK (disc_id_state <> 'not_cd_audio'
        OR (not_cd_rate IS NOT NULL AND audio_source IS NOT 'cd_rip')),
    CHECK ((disc_id_state = 'failed') = (disc_id_failure IS NOT NULL)),
    CHECK ((barcode_state = 'failed') = (barcode_failure IS NOT NULL)),
    CHECK ((text_state = 'failed') = (text_failure IS NOT NULL))
) STRICT;

-- The disc IDs, barcodes, catalog numbers and free-text candidates read off a
-- candidate, in reading order.
CREATE TABLE IF NOT EXISTS import_candidate_signal_value (
    content_hash TEXT,
    identify_run INTEGER,
    list         TEXT NOT NULL CHECK (list IN ('disc_id', 'barcode', 'catalog', 'free_text', 'isrc', 'track_title')),
    position     INTEGER NOT NULL CHECK (position >= 0),
    value        TEXT NOT NULL,
    -- The candidate-relative path of the file a barcode was read off, or of the
    -- LOG or CUE a disc ID was hashed from, so a surface can mark that file's
    -- row; NULL for a library release's stored files.
    origin_path  TEXT CHECK (origin_path IS NULL OR list IN ('disc_id', 'barcode')),
    UNIQUE (content_hash, list, position),
    UNIQUE (identify_run, list, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_signals (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (identify_run) REFERENCES import_candidate_signals (identify_run) ON DELETE CASCADE,
    CHECK ((content_hash IS NULL) <> (identify_run IS NULL))
) STRICT;

-- Every line of text read off a candidate, from which ranking reads the
-- folder's own claims.
CREATE TABLE IF NOT EXISTS import_candidate_text_line (
    content_hash  TEXT,
    identify_run  INTEGER,
    position      INTEGER NOT NULL CHECK (position >= 0),
    text          TEXT NOT NULL,
    origin        TEXT NOT NULL
        CHECK (origin IN ('cue_sheet', 'artwork', 'folder_name', 'filename', 'text_file',
                          'file_tag')),
    UNIQUE (content_hash, position),
    UNIQUE (identify_run, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_signals (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (identify_run) REFERENCES import_candidate_signals (identify_run) ON DELETE CASCADE,
    CHECK ((content_hash IS NULL) <> (identify_run IS NULL))
) STRICT;
