-- What the candidate's link claims, worked out in Rust by the write that
-- stores the link and again by each write that changes what it comes from: a
-- claimed release stored again, or the albums kept for a claimed release's
-- album (`release_group_album_link`). The rows go with the link.

-- The records the link describes the candidate by, in the order surfaces list
-- them: a linked pressing's, merged from its claimed releases as they are
-- stored by `bae_catalogs::source_release::claimed_records`, or a linked
-- album's albums.
CREATE TABLE IF NOT EXISTS import_candidate_claimed_record (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL CHECK (position >= 0),
    catalog      TEXT NOT NULL CHECK (catalog <> ''),
    kind         TEXT NOT NULL CHECK (kind IN ('pressing', 'album')),
    key          TEXT NOT NULL CHECK (key <> ''),
    -- The album a pressing's record files it under, where its catalog names
    -- one.
    album        TEXT,
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_release_link (content_hash) ON DELETE CASCADE,
    CHECK (kind = 'pressing' OR album IS NULL)
) STRICT;

-- The covers a linked pressing offers the picker, in the order offered, by
-- `bae_catalogs::source_release::pick_covers` over its claimed releases as
-- they are stored: the cover as JSON (a `RemoteCover`), and the catalog that
-- serves it, which the picker's filter reads.
CREATE TABLE IF NOT EXISTS import_candidate_offered_cover (
    content_hash TEXT NOT NULL,
    position     INTEGER NOT NULL CHECK (position >= 0),
    source       TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    cover        TEXT NOT NULL CHECK (json_valid(cover)),
    PRIMARY KEY (content_hash, position),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_release_link (content_hash) ON DELETE CASCADE
) STRICT;
