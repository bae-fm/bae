-- The release-level metadata draft the import will write. A candidate this
-- device did not import that has no row holds the blank draft of its files
-- and the folder's own cover, as a scan seeds a new candidate it reads no
-- tags of: a folder whose release was deleted is back in Found so, and the
-- first write to its draft stores the row. An imported candidate holds no
-- draft: the import deletes it, with its source, its cover and its link.
CREATE TABLE IF NOT EXISTS import_candidate_edit (
    content_hash   TEXT PRIMARY KEY,
    album_title    TEXT NOT NULL,
    album_year     TEXT NOT NULL,
    year           TEXT NOT NULL,
    -- Its label rows are its `import_candidate_edit_label` rows, its barcode
    -- rows its `import_candidate_edit_barcode` rows.
    -- Where the pressing was released: an ISO 3166-1 alpha-2 country code, or a
    -- `crate::pressing::Region` key for a region no current code names.
    country            TEXT CHECK (country IS NULL OR (length(country) = 2 AND country = upper(country))),
    region             TEXT CHECK (region IS NULL OR region <> ''),
    -- A JSON array of {"medium", "count"}, one per carrier in the record's
    -- order; empty where nothing is stated.
    media              TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(media) AND json_type(media) = 'array'),
    status             TEXT CHECK (status IS NULL OR status IN ('official', 'promotion', 'bootleg', 'pseudo_release', 'withdrawn', 'expunged', 'cancelled')),
    packaging          TEXT CHECK (packaging IS NULL OR packaging IN ('jewel_case', 'slim_jewel_case', 'digipak', 'cardboard_sleeve', 'other', 'keep_case', 'unpackaged', 'cassette_case', 'book', 'fatbox', 'snap_case', 'gatefold_cover', 'discbox_slider', 'super_jewel_box', 'digibook', 'plastic_sleeve', 'box', 'slidepack', 'snap_pack', 'metal_tin', 'longbox', 'clamshell_case', 'digifile', 'slipcase')),
    -- What Discogs says about the pressing that no column holds: a JSON array
    -- of `crate::pressing::DiscogsDetail` keys, each once, in its order.
    discogs_details    TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(discogs_details) AND json_type(discogs_details) = 'array'),
    -- Who wrote the draft: nobody (the blank one discovery creates), discovery
    -- from the folder's tags ('prefill'), an identification run, or a person.
    -- Which provenance goes with each is checked in code, since it spans this
    -- row and the provenance row.
    author         TEXT NOT NULL
        CHECK (author IN ('nobody', 'prefill', 'identification', 'person')),
    -- Whether the draft is blank, and whether it is complete and valid; written
    -- with the draft so the import list need not read every draft whole.
    draft_blank    INTEGER NOT NULL CHECK (draft_blank IN (0, 1)),
    draft_valid    INTEGER NOT NULL CHECK (draft_valid IN (0, 1)),
    CHECK (country IS NULL OR region IS NULL),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The draft's label rows as the form holds them, in order, each field as
-- typed, empty meaning unset: a row the person added and has not filled yet
-- is blank. A row keeps the ids the catalog that stated it knows its label
-- by while it keeps the name it stated.
CREATE TABLE IF NOT EXISTS import_candidate_edit_label (
    content_hash   TEXT NOT NULL,
    position       INTEGER NOT NULL CHECK (position >= 0),
    name           TEXT NOT NULL,
    catalog_number TEXT NOT NULL,
    musicbrainz_label_id TEXT CHECK (musicbrainz_label_id IS NULL OR musicbrainz_label_id <> ''),
    discogs_label_id     TEXT CHECK (discogs_label_id IS NULL OR discogs_label_id <> ''),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE
) STRICT;

-- The draft's barcode rows as the form holds them, in order, each as typed:
-- a row the person added and has not filled yet is blank.
CREATE TABLE IF NOT EXISTS import_candidate_edit_barcode (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL CHECK (position >= 0),
    barcode      TEXT NOT NULL,
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE
) STRICT;

-- The draft's album artists: a library artist the user picked ('picked'), or
-- a name a source or the user gave ('credit'). Which library artist a credit
-- is, if any, is decided each time the draft is read and again on import.
CREATE TABLE IF NOT EXISTS import_candidate_album_artist_assignment (
    content_hash          TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('picked', 'credit')),
    artist_id             TEXT,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    -- A picked artist's printed name: one of its `artist_credits`, NULL for
    -- its own name.
    credit_key            TEXT,
    -- A credit's printed name, where it is not `name`.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    join_phrase           TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    CHECK (
        (assignment_kind = 'picked' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL
            AND credited_as IS NULL)
        OR
        (assignment_kind = 'credit' AND artist_id IS NULL AND name IS NOT NULL AND name <> ''
            AND credit_key IS NULL)
    )
) STRICT;

CREATE INDEX IF NOT EXISTS import_candidate_album_artist_by_artist
    ON import_candidate_album_artist_assignment (artist_id);

-- The mediums of the draft, in order: what applying a source read the
-- folder's audio to be on.
CREATE TABLE IF NOT EXISTS import_candidate_medium (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL CHECK (position >= 1),
    medium       TEXT CHECK (medium IS NULL OR medium <> ''),
    side_count   INTEGER NOT NULL CHECK (side_count >= 1),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE
) STRICT;

-- The tracks of the draft, each bound to the file (or CUE slice) it plays and
-- placed on a side of one of the draft's mediums. `stated_length_ms` is the
-- length the source the draft was read from lists for the track, unstated
-- where it lists none or the draft was read from no catalog.
CREATE TABLE IF NOT EXISTS import_candidate_track (
    content_hash           TEXT NOT NULL,
    track_id               TEXT NOT NULL,
    position               INTEGER NOT NULL CHECK (position >= 0),
    title                  TEXT NOT NULL,
    artist_assignment_kind TEXT NOT NULL CHECK (artist_assignment_kind IN ('album_artists', 'explicit')),
    medium                 INTEGER NOT NULL CHECK (medium >= 1),
    side                   INTEGER NOT NULL CHECK (side >= 1),
    track_number           INTEGER NOT NULL CHECK (track_number >= 1),
    stated_length_ms       INTEGER CHECK (stated_length_ms IS NULL OR stated_length_ms >= 0),
    file_kind              TEXT NOT NULL CHECK (file_kind IN ('standalone', 'sheet_slice')),
    file_id                TEXT NOT NULL,
    sheet_id               TEXT,
    slice_index            INTEGER CHECK (slice_index IS NULL OR slice_index >= 0),
    PRIMARY KEY (content_hash, track_id),
    UNIQUE (content_hash, position),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (content_hash, medium) REFERENCES import_candidate_medium (content_hash, position),
    CHECK (
        (file_kind = 'standalone' AND file_id IS NOT NULL AND sheet_id IS NULL AND slice_index IS NULL)
        OR (file_kind = 'sheet_slice' AND file_id IS NOT NULL AND sheet_id IS NOT NULL AND slice_index IS NOT NULL)
    )
) STRICT;

-- The draft's per-track artists, where a track does not take the album's;
-- 'picked' and 'credit' as for the album's.
CREATE TABLE IF NOT EXISTS import_candidate_track_artist_assignment (
    content_hash          TEXT NOT NULL,
    track_id              TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('picked', 'credit')),
    artist_id             TEXT,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    -- A picked artist's printed name: one of its `artist_credits`, NULL for
    -- its own name.
    credit_key            TEXT,
    -- A credit's printed name, where it is not `name`.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    join_phrase           TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
    PRIMARY KEY (content_hash, track_id, position),
    FOREIGN KEY (content_hash, track_id)
        REFERENCES import_candidate_track (content_hash, track_id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    CHECK (
        (assignment_kind = 'picked' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL
            AND credited_as IS NULL)
        OR
        (assignment_kind = 'credit' AND artist_id IS NULL AND name IS NOT NULL AND name <> ''
            AND credit_key IS NULL)
    )
) STRICT;
