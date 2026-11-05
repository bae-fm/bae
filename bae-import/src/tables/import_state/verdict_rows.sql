-- What an identify run concluded, owned by the candidate's files
-- (`content_hash`); or how a run in flight stands, owned by the run
-- (`identify_run`): 'running' while its lookups are out, with the ledger as
-- it stands and the releases its answers so far combine to, then the answer
-- it reached while that is written. A run's rows go with its
-- import_identify_run row.
CREATE TABLE IF NOT EXISTS import_candidate_verdict (
    content_hash  TEXT UNIQUE,
    identify_run  INTEGER UNIQUE,
    kind          TEXT NOT NULL
        CHECK (kind IN ('running', 'found', 'not_found', 'manual_only', 'failed', 'error')),
    -- How many tracks the folder played when the verdict was reached.
    track_count   INTEGER CHECK (track_count IS NULL OR track_count >= 0),
    -- The lookup failures of a failed verdict, stored as JSON since no query
    -- reads into them.
    failures_json TEXT CHECK (
        failures_json IS NULL
        OR (json_valid(failures_json)
            AND json_type(failures_json) = 'array'
            AND json_array_length(failures_json) > 0)
    ),
    -- How each clue stood in the ledger the run recorded as it ended, or as
    -- it stands for a run in flight, all NULL when it recorded none. The
    -- ledger's rows and lookups are import_candidate_lookup_value and the
    -- tables beside it. 'absent': no LOG or CUE, no barcode source, no ISRC.
    -- 'not_cd_audio': the CUE lays out audio at a rate no CD has.
    -- 'no_codes': a barcode source held no code. 'none_found': no catalog
    -- number was offered or in effect. 'not_needed': the clues answered, so
    -- no title search went out; 'no_title': there was no title to search by.
    -- Only a run in flight stands at 'reading' (extraction has not reported
    -- the disc IDs or the ISRCs), 'scanning' (the artwork is still being read,
    -- so more barcodes or catalog numbers may come) or 'waiting' (the title
    -- search goes out once the clues have answered).
    disc_id_step  TEXT CHECK (disc_id_step IN ('reading', 'absent', 'not_cd_audio', 'read')),
    barcode_step  TEXT CHECK (barcode_step IN ('absent', 'no_codes', 'rows', 'scanning')),
    catalog_step  TEXT CHECK (catalog_step IN ('none_found', 'numbers', 'scanning')),
    isrc_step     TEXT CHECK (isrc_step IN ('reading', 'absent', 'read')),
    search_step   TEXT CHECK (search_step IN ('not_needed', 'no_title', 'waiting', 'searched')),
    -- The words the title search asked, or will ask, for a 'searched' or
    -- 'waiting' step: the album, and the album artist, blank where the draft
    -- named none.
    search_album  TEXT CHECK (search_album <> ''),
    search_artist TEXT,
    -- How bae broke on its own side, for an 'error' verdict: the error chain
    -- the person is shown.
    error_detail  TEXT CHECK (error_detail IS NULL OR error_detail <> ''),
    -- The person kept their own draft over what this verdict offered: none of
    -- its releases is the folder's, or it found none. It goes with the
    -- verdict, so the next identification starts without it.
    kept_own_draft INTEGER NOT NULL CHECK (kept_own_draft IN (0, 1)),
    -- The folder's own files rule out every row found: a CD rip where no row
    -- could be a CD, or a sample rate no CD holds where every row is a CD.
    -- Such a verdict is never Ready.
    medium_conflict TEXT CHECK (medium_conflict IS NULL OR medium_conflict IN ('cd_rip', 'not_cd_audio')),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (identify_run) REFERENCES import_identify_run (identify_run) ON DELETE CASCADE,
    CHECK ((content_hash IS NULL) <> (identify_run IS NULL)),
    CHECK (kind <> 'running' OR identify_run IS NOT NULL),
    CHECK (identify_run IS NULL OR kept_own_draft = 0),
    -- A run in flight always lays its clues out; a run that ended recorded
    -- them settled.
    CHECK (kind <> 'running' OR disc_id_step IS NOT NULL),
    CHECK (kind = 'running'
           OR (disc_id_step IS NOT 'reading' AND barcode_step IS NOT 'scanning'
               AND catalog_step IS NOT 'scanning' AND isrc_step IS NOT 'reading'
               AND search_step IS NOT 'waiting')),
    CHECK ((kind IN ('not_found', 'error')) = (track_count IS NULL)),
    CHECK ((kind = 'failed') = (failures_json IS NOT NULL)),
    CHECK ((kind = 'error') = (error_detail IS NOT NULL)),
    CHECK (medium_conflict IS NULL OR kind IN ('found', 'failed')),
    CHECK ((disc_id_step IS NULL) = (barcode_step IS NULL)
           AND (disc_id_step IS NULL) = (catalog_step IS NULL)
           AND (disc_id_step IS NULL) = (isrc_step IS NULL)
           AND (disc_id_step IS NULL) = (search_step IS NULL)),
    CHECK (kind <> 'error' OR disc_id_step IS NULL),
    CHECK ((search_step IS 'searched' OR search_step IS 'waiting') = (search_album IS NOT NULL)),
    CHECK ((search_album IS NULL) = (search_artist IS NULL))
) STRICT;
