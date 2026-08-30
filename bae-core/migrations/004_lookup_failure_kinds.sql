-- Rate limits and rejected credentials are provider outcomes, not generic
-- provider errors. Rebuild the two-table signal graph so its closed failure
-- set can store those distinctions without losing any settled candidate.

ALTER TABLE import_candidate_signal_value RENAME TO import_candidate_signal_value_v3;
ALTER TABLE import_candidate_signals RENAME TO import_candidate_signals_v3;

CREATE TABLE import_candidate_signals (
    content_hash        TEXT PRIMARY KEY,
    disc_id_state       TEXT NOT NULL CHECK (disc_id_state IN ('computed', 'absent', 'failed')),
    disc_id             TEXT,
    disc_id_source_file TEXT,
    track_count         INTEGER NOT NULL CHECK (track_count >= 0),
    disc_id_failure     TEXT CHECK (disc_id_failure IS NULL OR disc_id_failure IN ('network', 'provider', 'rate_limited', 'credentials', 'timeout', 'artwork_analysis', 'diagnostic')),
    disc_id_failure_status INTEGER,
    disc_id_failure_detail TEXT,
    barcode_state       TEXT NOT NULL CHECK (barcode_state IN ('settled', 'failed', 'absent')),
    barcode_failure     TEXT CHECK (barcode_failure IS NULL OR barcode_failure IN ('network', 'provider', 'rate_limited', 'credentials', 'timeout', 'artwork_analysis', 'diagnostic')),
    barcode_failure_status INTEGER,
    barcode_failure_detail TEXT,
    text_state          TEXT NOT NULL CHECK (text_state IN ('settled', 'failed')),
    text_failure        TEXT CHECK (text_failure IS NULL OR text_failure IN ('network', 'provider', 'rate_limited', 'credentials', 'timeout', 'artwork_analysis', 'diagnostic')),
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

INSERT INTO import_candidate_signals SELECT * FROM import_candidate_signals_v3;

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

INSERT INTO import_candidate_signal_value SELECT * FROM import_candidate_signal_value_v3;

DROP TABLE import_candidate_signal_value_v3;
DROP TABLE import_candidate_signals_v3;
