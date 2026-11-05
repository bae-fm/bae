-- Where each candidate something is running for stands right now, keyed as
-- the import list keys it. This table is the only place that holds it: every
-- change is a write its owner awaits, and the guards that decide what may
-- happen next read it in the write they guard.
--
-- Its identification is of the files it names (`content_hash`,
-- `file_edit_revision`, as `scan_candidate` holds them), and ends in the write
-- that leaves the scan holding no candidate with those files, or in the write
-- of a decision about the candidate: queued by an admission, running as
-- `identify_run`, having that run's answer written (the release it picked
-- unattended read first, and whether an Import was asked of that answer), or
-- the answer's write failed. Its import is the one `import_id`
-- names, at the step `import_step` names: queued for the worker, validating
-- its source files, fetching artist pictures, reading files, measuring
-- loudness, or finalizing (writing its release), each step written by the
-- worker before it starts it; a second import is refused while one stands. A
-- row exists only while one of the two is set. No run or import outlives the
-- process, so the table is emptied as the library opens. How far through its
-- step an import is is not here: it moves too often to be written.
CREATE TABLE IF NOT EXISTS import_candidate_standing (
    candidate_key      BLOB PRIMARY KEY,
    identification     TEXT CHECK (identification IN (
        'queued', 'running', 'finalizing', 'reading_pick', 'not_written', 'inapplicable')),
    content_hash       TEXT,
    file_edit_revision INTEGER CHECK (file_edit_revision >= 0),
    admission          TEXT CHECK (admission IN ('automatic', 'requested')),
    identify_run       INTEGER CHECK (identify_run >= 0),
    import_asked       INTEGER CHECK (import_asked IN (0, 1)),
    save_failure_error TEXT,
    import_step        TEXT CHECK (import_step IN (
        'queued', 'validating_source_files', 'fetching_artist_pictures',
        'reading_files', 'measuring_loudness', 'finalizing')),
    import_id          TEXT,
    CHECK (identification IS NOT NULL OR import_step IS NOT NULL),
    CHECK ((identification IS NOT NULL) = (content_hash IS NOT NULL)),
    CHECK ((identification IS NOT NULL) = (file_edit_revision IS NOT NULL)),
    CHECK ((identification IS 'queued') = (admission IS NOT NULL)),
    CHECK ((identification IN ('running', 'finalizing', 'reading_pick') IS 1)
        = (identify_run IS NOT NULL)),
    CHECK ((identification IS 'reading_pick') = (import_asked IS NOT NULL)),
    CHECK ((identification IN ('not_written', 'inapplicable') IS 1)
        = (save_failure_error IS NOT NULL)),
    CHECK ((import_step IS NOT NULL) = (import_id IS NOT NULL))
) STRICT;

-- The rows an import holds, which the count of imports in flight reads.
CREATE INDEX IF NOT EXISTS idx_import_candidate_standing_import
    ON import_candidate_standing (import_step) WHERE import_step IS NOT NULL;

-- The identifications counted as one batch, for the count the import list
-- draws: each candidate admitted since the batch opened, and whether its
-- identification has ended. Every change to a candidate's identification
-- moves it in that change's own write; the batch is over, and its rows go,
-- once every one has ended. Emptied as the library opens, with the standings.
CREATE TABLE IF NOT EXISTS import_identification_batch (
    candidate_key BLOB PRIMARY KEY,
    ended         INTEGER NOT NULL CHECK (ended IN (0, 1))
) STRICT;
