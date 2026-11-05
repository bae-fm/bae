-- Every release a run's lookups returned, in listed order, with what the
-- record said and which lookup found it: the releases it offers, then the
-- ones agreement set aside, then the ones a lookup of the recorded ledger
-- found that neither lists. A typed search's releases are its rows too, each
-- under the catalog part that returned it.
-- This table and the ledger's (import_candidate_lookup_*) hang off their
-- verdict row by its owner: the candidate's files, or a run in flight; a
-- search's hang off its catalog part, by `search_run`.
CREATE TABLE IF NOT EXISTS import_candidate_match (
    content_hash        TEXT,
    identify_run        INTEGER,
    search_run          INTEGER,
    -- 'done' for a search's release, naming the part that answered with it,
    -- so a part only holds releases while it stands answered; NULL otherwise.
    search_state        TEXT CHECK (search_state = 'done'),
    position            INTEGER NOT NULL CHECK (position >= 0),
    -- 'offered': a release of the rows the run offers. 'narrowed_out': one of
    -- the rows agreement set aside. 'unlisted': listed in neither, because
    -- its tracklist holds other tracks than the folder or it looks alike an
    -- offered row; only the ledger's cells show it. 'returned': a release a
    -- typed search's catalog returned.
    listing             TEXT NOT NULL
        CHECK (listing IN ('offered', 'narrowed_out', 'unlisted', 'returned')),
    -- The pressing row this release belongs to, numbered from zero; offered
    -- and narrowed-out releases each number their own rows. NULL for an
    -- unlisted or returned release, which is in no row.
    pressing            INTEGER CHECK (pressing >= 0),
    -- For a search's release, whether a pick read its documents, which then
    -- state its album on the other catalog in `album_links`; NULL otherwise.
    picked              INTEGER CHECK (picked IN (0, 1)),
    source              TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id          TEXT NOT NULL,
    title               TEXT NOT NULL,
    artist              TEXT,
    year                INTEGER,
    -- Its labels are its import_candidate_match_label rows.
    -- Where the pressing was released: an ISO 3166-1 alpha-2 country code, or a
    -- `crate::pressing::Region` key for a region no current code names.
    country            TEXT CHECK (country IS NULL OR (length(country) = 2 AND country = upper(country))),
    region             TEXT CHECK (region IS NULL OR region <> ''),
    status             TEXT CHECK (status IS NULL OR status IN ('official', 'promotion', 'bootleg', 'pseudo_release', 'withdrawn', 'expunged', 'cancelled')),
    packaging          TEXT CHECK (packaging IS NULL OR packaging IN ('jewel_case', 'slim_jewel_case', 'digipak', 'cardboard_sleeve', 'other', 'keep_case', 'unpackaged', 'cassette_case', 'book', 'fatbox', 'snap_case', 'gatefold_cover', 'discbox_slider', 'super_jewel_box', 'digibook', 'plastic_sleeve', 'box', 'slidepack', 'snap_pack', 'metal_tin', 'longbox', 'clamshell_case', 'digifile', 'slipcase')),
    -- What Discogs says about the pressing that no column holds: a JSON array
    -- of `crate::pressing::DiscogsDetail` keys, each once, in its order.
    discogs_details    TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(discogs_details) AND json_type(discogs_details) = 'array'),
    -- 'undescribed': the record described no media and there are no medium
    -- rows. 'per_medium': one row per medium listed, its medium NULL where bae
    -- does not know the carrier. 'formats': one row per format entry that is a
    -- medium, with its quantity.
    media_kind          TEXT NOT NULL
        CHECK (media_kind IN ('undescribed', 'per_medium', 'formats')),
    -- The lead cover's original; its downscaled copies are in
    -- import_candidate_match_cover_copy.
    cover_url           TEXT,
    cover_label         TEXT,
    cover_source        TEXT CHECK (cover_source IS NULL OR cover_source IN ('musicbrainz', 'discogs')),
    -- 'stated': the record's catalog says the image exists. 'unstated': an
    -- address the record said nothing about.
    cover_standing      TEXT CHECK (cover_standing IS NULL OR cover_standing IN ('stated', 'unstated')),
    source_group_id     TEXT,
    -- What the record's catalog says its album is on the other catalog.
    -- 'not_asked': never read. 'read': the album link rows hold what it named.
    -- 'unread': a needed document could not be fetched and nothing named an
    -- album.
    album_links         TEXT NOT NULL CHECK (album_links IN ('not_asked', 'read', 'unread')),
    -- NULL until the source is asked for its tracklist. 'unreadable': it
    -- lists tracks for the audio that cannot be read into values.
    source_tracks_kind  TEXT CHECK (source_tracks_kind IS NULL OR source_tracks_kind IN ('listed', 'nothing', 'unreadable')),
    source_tracks_count INTEGER CHECK (source_tracks_count IS NULL OR source_tracks_count >= 0),
    -- Why the release's full document could not be read when the run offered
    -- its row, which then holds what the lookup returned. NULL when it was
    -- read or never asked for.
    document_failure        TEXT CHECK (document_failure IS NULL OR document_failure IN ('network', 'provider', 'timeout')),
    document_failure_status INTEGER,
    -- The note of this release the folder's text names its row by, where the
    -- ranking's notes point went to that row; NULL otherwise, and always for a
    -- release the run does not offer.
    named_note              TEXT CHECK (named_note IS NULL OR named_note <> ''),
    UNIQUE (content_hash, position),
    UNIQUE (identify_run, position),
    UNIQUE (search_run, position),
    -- Referenced by the medium rows with the media kind, so a medium row always
    -- belongs to a match of its kind.
    UNIQUE (content_hash, position, media_kind),
    UNIQUE (identify_run, position, media_kind),
    UNIQUE (search_run, position, media_kind),
    -- Referenced by the cover copies; only a match with a cover has one.
    UNIQUE (content_hash, position, cover_url),
    UNIQUE (identify_run, position, cover_url),
    UNIQUE (search_run, position, cover_url),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_verdict (content_hash) ON DELETE CASCADE,
    FOREIGN KEY (identify_run) REFERENCES import_candidate_verdict (identify_run) ON DELETE CASCADE,
    FOREIGN KEY (search_run, source, search_state)
        REFERENCES import_candidate_search_source (search_run, catalog, state) ON DELETE CASCADE,
    CHECK ((cover_url IS NULL) = (cover_label IS NULL) AND (cover_url IS NULL) = (cover_source IS NULL) AND (cover_url IS NULL) = (cover_standing IS NULL)),
    CHECK ((source_tracks_kind = 'listed') = (source_tracks_count IS NOT NULL)),
    CHECK (listing = 'offered' OR named_note IS NULL),
    CHECK ((listing IN ('unlisted', 'returned')) = (pressing IS NULL)),
    CHECK (country IS NULL OR region IS NULL),
    CHECK (document_failure_status IS NULL OR document_failure = 'provider'),
    CHECK ((content_hash IS NOT NULL) + (identify_run IS NOT NULL) + (search_run IS NOT NULL) = 1),
    CHECK ((search_run IS NOT NULL) = (search_state IS NOT NULL)),
    CHECK ((search_run IS NOT NULL) = (listing = 'returned')),
    CHECK ((search_run IS NOT NULL) = (picked IS NOT NULL)),
    -- A search's release states its album only once a pick read it.
    CHECK (picked IS NOT 0 OR album_links = 'not_asked')
) STRICT;

-- The rows naming a catalog release, which keep it stored where it was read.
CREATE INDEX IF NOT EXISTS idx_import_candidate_match_release
    ON import_candidate_match (source, release_id);

-- Every label a matched record is on, in its order, each stating a name, a
-- catalog number or both.
CREATE TABLE IF NOT EXISTS import_candidate_match_label (
    content_hash   TEXT,
    identify_run   INTEGER,
    search_run     INTEGER,
    position       INTEGER NOT NULL,
    ordinal        INTEGER NOT NULL CHECK (ordinal >= 0),
    name           TEXT CHECK (name IS NULL OR name <> ''),
    catalog_number TEXT CHECK (catalog_number IS NULL OR catalog_number <> ''),
    -- The ids the catalogs know a named label by.
    musicbrainz_label_id TEXT CHECK (musicbrainz_label_id IS NULL OR musicbrainz_label_id <> ''),
    discogs_label_id     TEXT CHECK (discogs_label_id IS NULL OR discogs_label_id <> ''),
    CHECK (name IS NOT NULL OR (musicbrainz_label_id IS NULL AND discogs_label_id IS NULL)),
    UNIQUE (content_hash, position, ordinal),
    UNIQUE (identify_run, position, ordinal),
    UNIQUE (search_run, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match (content_hash, position) ON DELETE CASCADE,
    FOREIGN KEY (identify_run, position)
        REFERENCES import_candidate_match (identify_run, position) ON DELETE CASCADE,
    FOREIGN KEY (search_run, position)
        REFERENCES import_candidate_match (search_run, position) ON DELETE CASCADE,
    CHECK (name IS NOT NULL OR catalog_number IS NOT NULL),
    CHECK ((content_hash IS NOT NULL) + (identify_run IS NOT NULL) + (search_run IS NOT NULL) = 1)
) STRICT;

-- Every barcode a matched record states.
CREATE TABLE IF NOT EXISTS import_candidate_match_barcode (
    content_hash TEXT,
    identify_run INTEGER,
    search_run   INTEGER,
    position     INTEGER NOT NULL,
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    barcode      TEXT NOT NULL CHECK (barcode <> ''),
    UNIQUE (content_hash, position, ordinal),
    UNIQUE (identify_run, position, ordinal),
    UNIQUE (search_run, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match (content_hash, position) ON DELETE CASCADE,
    FOREIGN KEY (identify_run, position)
        REFERENCES import_candidate_match (identify_run, position) ON DELETE CASCADE,
    FOREIGN KEY (search_run, position)
        REFERENCES import_candidate_match (search_run, position) ON DELETE CASCADE,
    CHECK ((content_hash IS NOT NULL) + (identify_run IS NOT NULL) + (search_run IS NOT NULL) = 1)
) STRICT;

-- The downscaled copies the catalog serves of a match's cover, one per size (a
-- copy's longer side is at most max_edge pixels).
CREATE TABLE IF NOT EXISTS import_candidate_match_cover_copy (
    content_hash TEXT,
    identify_run INTEGER,
    search_run   INTEGER,
    position     INTEGER NOT NULL,
    cover_url    TEXT NOT NULL,
    max_edge     INTEGER NOT NULL CHECK (max_edge > 0),
    url          TEXT NOT NULL CHECK (url <> ''),
    UNIQUE (content_hash, position, max_edge),
    UNIQUE (identify_run, position, max_edge),
    UNIQUE (search_run, position, max_edge),
    FOREIGN KEY (content_hash, position, cover_url)
        REFERENCES import_candidate_match (content_hash, position, cover_url) ON DELETE CASCADE,
    FOREIGN KEY (identify_run, position, cover_url)
        REFERENCES import_candidate_match (identify_run, position, cover_url) ON DELETE CASCADE,
    FOREIGN KEY (search_run, position, cover_url)
        REFERENCES import_candidate_match (search_run, position, cover_url) ON DELETE CASCADE,
    CHECK ((content_hash IS NOT NULL) + (identify_run IS NOT NULL) + (search_run IS NOT NULL) = 1)
) STRICT;

-- Every other catalog entry a matched record links to.
CREATE TABLE IF NOT EXISTS import_candidate_match_link (
    content_hash TEXT,
    identify_run INTEGER,
    search_run   INTEGER,
    position     INTEGER NOT NULL,
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    catalog      TEXT NOT NULL CHECK (catalog <> ''),
    key          TEXT NOT NULL CHECK (key <> ''),
    UNIQUE (content_hash, position, ordinal),
    UNIQUE (identify_run, position, ordinal),
    UNIQUE (search_run, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match (content_hash, position) ON DELETE CASCADE,
    FOREIGN KEY (identify_run, position)
        REFERENCES import_candidate_match (identify_run, position) ON DELETE CASCADE,
    FOREIGN KEY (search_run, position)
        REFERENCES import_candidate_match (search_run, position) ON DELETE CASCADE,
    CHECK ((content_hash IS NOT NULL) + (identify_run IS NOT NULL) + (search_run IS NOT NULL) = 1)
) STRICT;

-- Every other catalog's album a statement names as a matched record's album
-- (the Discogs masters a MusicBrainz release group is), for a match whose
-- album_links is 'read', in the order the statements were read, strongest
-- first. Which statement named it is not kept.
CREATE TABLE IF NOT EXISTS import_candidate_match_album_link (
    content_hash        TEXT,
    identify_run        INTEGER,
    search_run          INTEGER,
    position            INTEGER NOT NULL,
    ordinal             INTEGER NOT NULL CHECK (ordinal >= 0),
    catalog             TEXT NOT NULL CHECK (catalog <> ''),
    key                 TEXT NOT NULL CHECK (key <> ''),
    UNIQUE (content_hash, position, ordinal),
    UNIQUE (identify_run, position, ordinal),
    UNIQUE (search_run, position, ordinal),
    FOREIGN KEY (content_hash, position)
        REFERENCES import_candidate_match (content_hash, position) ON DELETE CASCADE,
    FOREIGN KEY (identify_run, position)
        REFERENCES import_candidate_match (identify_run, position) ON DELETE CASCADE,
    FOREIGN KEY (search_run, position)
        REFERENCES import_candidate_match (search_run, position) ON DELETE CASCADE,
    CHECK ((content_hash IS NOT NULL) + (identify_run IS NOT NULL) + (search_run IS NOT NULL) = 1)
) STRICT;

-- A matched record's media, one row per medium or format entry as the match's
-- media kind says: the carrier (a `crate::pressing::Medium` key, NULL where bae
-- does not know it) and how many.
CREATE TABLE IF NOT EXISTS import_candidate_match_medium (
    content_hash TEXT,
    identify_run INTEGER,
    search_run   INTEGER,
    position     INTEGER NOT NULL,
    media_kind   TEXT NOT NULL CHECK (media_kind IN ('per_medium', 'formats')),
    ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
    medium       TEXT CHECK (medium IS NULL OR medium <> ''),
    quantity     INTEGER NOT NULL CHECK (quantity >= 1),
    UNIQUE (content_hash, position, ordinal),
    UNIQUE (identify_run, position, ordinal),
    UNIQUE (search_run, position, ordinal),
    FOREIGN KEY (content_hash, position, media_kind)
        REFERENCES import_candidate_match (content_hash, position, media_kind)
        ON DELETE CASCADE,
    FOREIGN KEY (identify_run, position, media_kind)
        REFERENCES import_candidate_match (identify_run, position, media_kind)
        ON DELETE CASCADE,
    FOREIGN KEY (search_run, position, media_kind)
        REFERENCES import_candidate_match (search_run, position, media_kind)
        ON DELETE CASCADE,
    CHECK (media_kind = 'formats' OR quantity = 1),
    CHECK ((content_hash IS NOT NULL) + (identify_run IS NOT NULL) + (search_run IS NOT NULL) = 1)
) STRICT;
