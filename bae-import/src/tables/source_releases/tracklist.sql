-- One row of a medium's tracklist. `entry` numbers the release's rows in
-- tracklist order, a Discogs index's sub-tracks right after it with the index
-- as parent. A 'heading' titles the rows after it.
CREATE TABLE IF NOT EXISTS source_release_entry (
    catalog     TEXT NOT NULL,
    release_id  TEXT NOT NULL,
    entry       INTEGER NOT NULL CHECK (entry >= 0),
    medium      INTEGER NOT NULL,
    parent      INTEGER CHECK (parent IS NULL OR parent < entry),
    kind        TEXT NOT NULL CHECK (kind IN ('track', 'heading', 'index')),
    -- The position the catalog prints ('A1', '1-2'); NULL where it prints none.
    position    TEXT CHECK (position IS NULL OR position <> ''),
    -- NULL for a MusicBrainz track with no usable title, which reading
    -- refuses.
    title       TEXT,
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    PRIMARY KEY (catalog, release_id, entry),
    FOREIGN KEY (catalog, release_id, medium)
        REFERENCES source_release_medium (catalog, release_id, position) ON DELETE CASCADE,
    FOREIGN KEY (catalog, release_id, parent)
        REFERENCES source_release_entry (catalog, release_id, entry) ON DELETE CASCADE,
    CHECK (kind = 'track' OR catalog = 'discogs'),
    CHECK (parent IS NULL OR catalog = 'discogs')
) STRICT;

-- A row's display credits at their positions among its credits. The printed
-- name heads a picker's row; the artist, where the catalog names one, is who
-- is credited.
CREATE TABLE IF NOT EXISTS source_release_entry_credit (
    catalog               TEXT NOT NULL,
    release_id            TEXT NOT NULL,
    entry                 INTEGER NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    credited_name         TEXT NOT NULL,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    -- What the catalog prints after the credit when another follows.
    join_phrase           TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
    PRIMARY KEY (catalog, release_id, entry, position),
    FOREIGN KEY (catalog, release_id, entry)
        REFERENCES source_release_entry (catalog, release_id, entry) ON DELETE CASCADE,
    CHECK (name IS NOT NULL OR (sort_name IS NULL AND musicbrainz_artist_id IS NULL
        AND discogs_artist_id IS NULL))
) STRICT;

-- A row's credits in a role, at their positions among its relations or
-- credits.
CREATE TABLE IF NOT EXISTS source_release_entry_role (
    catalog               TEXT NOT NULL,
    release_id            TEXT NOT NULL,
    entry                 INTEGER NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    -- The name the catalog prints for the artist where it is another than the
    -- artist's own.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    -- What the artist did: a `bae_types::credit_role::CreditRoleKind` key,
    -- 'performer' (its part, if the catalog names one, in `role_words`), or
    -- 'stated' for the catalog's own words (`role_words`), with the role's
    -- qualifiers as a JSON list.
    role                  TEXT NOT NULL CHECK (role IN ('writer', 'composer', 'lyricist', 'librettist', 'translator', 'reviser', 'scriptwriter', 'reconstructor', 'arranger', 'producer', 'engineer', 'mixer', 'mastering', 'performer', 'stated')),
    role_words            TEXT CHECK (role_words IS NULL OR role_words <> ''),
    qualifiers            TEXT NOT NULL CHECK (json_valid(qualifiers) AND json_type(qualifiers) = 'array'),
    CHECK (CASE role WHEN 'stated' THEN role_words IS NOT NULL WHEN 'performer' THEN 1 ELSE role_words IS NULL END),
    PRIMARY KEY (catalog, release_id, entry, position),
    FOREIGN KEY (catalog, release_id, entry)
        REFERENCES source_release_entry (catalog, release_id, entry) ON DELETE CASCADE
) STRICT;

-- The MusicBrainz works a track performs: a work hangs off its track at its
-- position among the recording's relations; a part hangs off its work at its
-- position among that work's relations, in the relation's direction.
CREATE TABLE IF NOT EXISTS source_release_work (
    catalog             TEXT NOT NULL CHECK (catalog = 'musicbrainz'),
    release_id          TEXT NOT NULL,
    node                INTEGER NOT NULL CHECK (node >= 0),
    entry               INTEGER,
    parent              INTEGER CHECK (parent IS NULL OR parent < node),
    position            INTEGER NOT NULL CHECK (position >= 0),
    direction           TEXT CHECK (direction IS NULL OR direction IN ('forward', 'backward')),
    musicbrainz_work_id TEXT NOT NULL,
    title               TEXT NOT NULL,
    disambiguation      TEXT,
    work_type           TEXT,
    PRIMARY KEY (catalog, release_id, node),
    FOREIGN KEY (catalog, release_id, entry)
        REFERENCES source_release_entry (catalog, release_id, entry) ON DELETE CASCADE,
    FOREIGN KEY (catalog, release_id, parent)
        REFERENCES source_release_work (catalog, release_id, node) ON DELETE CASCADE,
    CHECK ((entry IS NULL) <> (parent IS NULL)),
    CHECK ((parent IS NULL) = (direction IS NULL))
) STRICT;

-- A work's credits, at their positions among the work's relations.
CREATE TABLE IF NOT EXISTS source_release_work_credit (
    catalog               TEXT NOT NULL,
    release_id            TEXT NOT NULL,
    node                  INTEGER NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    -- The name the catalog prints for the artist where it is another than the
    -- artist's own.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    -- What the artist did: a `bae_types::credit_role::CreditRoleKind` key,
    -- 'performer' (its part, if the catalog names one, in `role_words`), or
    -- 'stated' for the catalog's own words (`role_words`), with the role's
    -- qualifiers as a JSON list.
    role                  TEXT NOT NULL CHECK (role IN ('writer', 'composer', 'lyricist', 'librettist', 'translator', 'reviser', 'scriptwriter', 'reconstructor', 'arranger', 'producer', 'engineer', 'mixer', 'mastering', 'performer', 'stated')),
    role_words            TEXT CHECK (role_words IS NULL OR role_words <> ''),
    qualifiers            TEXT NOT NULL CHECK (json_valid(qualifiers) AND json_type(qualifiers) = 'array'),
    CHECK (CASE role WHEN 'stated' THEN role_words IS NOT NULL WHEN 'performer' THEN 1 ELSE role_words IS NULL END),
    PRIMARY KEY (catalog, release_id, node, position),
    FOREIGN KEY (catalog, release_id, node)
        REFERENCES source_release_work (catalog, release_id, node) ON DELETE CASCADE
) STRICT;
