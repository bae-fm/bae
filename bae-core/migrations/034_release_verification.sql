-- What the rip databases said about a release's audio. A rip looks each track
-- up in AccurateRip — and, where the ripper supports it, in the CUETools
-- database — and each answers with how many other people's copies of that disc
-- carry the same bits. One row per track: the counts, and the CRC of the audio
-- those counts are about.
--
-- Not what a catalog says about the release (that is a record), and not what
-- the folder itself states (that is a mark): this is other people's reading of
-- the same disc.
CREATE TABLE release_verification (
    id                      TEXT NOT NULL PRIMARY KEY,
    release_id              TEXT NOT NULL,
    track                   INTEGER NOT NULL CHECK (track >= 1),
    -- Where the counts came from. Reading the log the rip left beside the
    -- audio is one source; asking the databases directly is another.
    source                  TEXT NOT NULL CHECK (source IN ('log')),
    -- How many other copies AccurateRip holds that agree with these bits. NULL
    -- where it holds none, disagreed, or was never asked: a disagreement's
    -- count belongs to the copy the database held, not to this rip.
    accuraterip_confidence  INTEGER CHECK (accuraterip_confidence IS NULL OR accuraterip_confidence >= 0),
    ctdb_confidence         INTEGER CHECK (ctdb_confidence IS NULL OR ctdb_confidence >= 0),
    -- The checksum of the bits that were kept, where the source states one.
    crc                     INTEGER,
    _updated_at             TEXT NOT NULL,
    created_at              TEXT NOT NULL,
    UNIQUE (release_id, track),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX idx_release_verification_release ON release_verification (release_id);

-- The same reading, held with the candidate the extraction pass read it off,
-- so the commit can carry it to the release it becomes. Device-local like the
-- rest of candidate state: an import in progress is not a library fact.
--
-- The source sits on the candidate's signals header rather than on each row:
-- one extraction pass reads one log, so a candidate's tracks are all verified
-- the same way.
ALTER TABLE import_candidate_signals
    ADD COLUMN verification_source TEXT
    CHECK (verification_source IS NULL OR verification_source IN ('log'));

CREATE TABLE import_candidate_verification (
    content_hash            TEXT NOT NULL,
    track                   INTEGER NOT NULL CHECK (track >= 1),
    accuraterip_confidence  INTEGER CHECK (accuraterip_confidence IS NULL OR accuraterip_confidence >= 0),
    ctdb_confidence         INTEGER CHECK (ctdb_confidence IS NULL OR ctdb_confidence >= 0),
    crc                     INTEGER,
    PRIMARY KEY (content_hash, track),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_signals (content_hash) ON DELETE CASCADE
) STRICT;
