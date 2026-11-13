-- One catalog release bae fetched, with every fact the import reads from it
-- extracted at fetch time. Device-local, since any device can fetch it; a new
-- fetch replaces every row under it at once. A release is stored only while
-- something names it: a match row of a verdict, a run or a search
-- (import_candidate_match), a draft read from it
-- (import_candidate_draft_provenance), a link (import_candidate_release_link)
-- or a linked pressing's partner (import_candidate_release_link_partner). It
-- is stored in the write that writes the first row naming it, and the
-- triggers below delete it in the write that drops the last.
CREATE TABLE IF NOT EXISTS source_release (
    catalog            TEXT NOT NULL CHECK (catalog IN ('musicbrainz', 'discogs')),
    release_id         TEXT NOT NULL CHECK (release_id <> ''),
    -- The album its catalog files it under: a MusicBrainz release group or a
    -- Discogs master.
    source_group_id    TEXT,
    -- The album's facts: the release's own, or where it states none, those of
    -- its cross-referenced release and its album's documents. Its year reads
    -- as its first year, else the pressing's year.
    album_title        TEXT NOT NULL,
    -- The year the album first came out, as its release group's first release
    -- date or its master's year states it; never a pressing's year.
    album_first_year   INTEGER,
    -- The pressing's facts, filled the same way.
    year               INTEGER,
    -- Its labels are its source_release_label rows.
    -- Where the pressing was released: an ISO 3166-1 alpha-2 country code, or a
    -- `crate::pressing::Region` key for a region no current code names.
    country            TEXT CHECK (country IS NULL OR (length(country) = 2 AND country = upper(country))),
    region             TEXT CHECK (region IS NULL OR region <> ''),
    -- Its barcodes are its source_release_barcode rows. Its media are counted
    -- from source_release_medium (MusicBrainz) or source_release_format
    -- (Discogs) on read.
    status             TEXT CHECK (status IS NULL OR status IN ('official', 'promotion', 'bootleg', 'pseudo_release', 'withdrawn', 'expunged', 'cancelled')),
    packaging          TEXT CHECK (packaging IS NULL OR packaging IN ('jewel_case', 'slim_jewel_case', 'digipak', 'cardboard_sleeve', 'other', 'keep_case', 'unpackaged', 'cassette_case', 'book', 'fatbox', 'snap_case', 'gatefold_cover', 'discbox_slider', 'super_jewel_box', 'digibook', 'plastic_sleeve', 'box', 'slidepack', 'snap_pack', 'metal_tin', 'longbox', 'clamshell_case', 'digifile', 'slipcase')),
    -- What Discogs says about the pressing that no column holds: a JSON array
    -- of `crate::pressing::DiscogsDetail` keys, each once, in its order.
    discogs_details    TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(discogs_details) AND json_type(discogs_details) = 'array'),
    -- What the release's own document writes about which pressing it is, in
    -- free text: a JSON array of strings — a MusicBrainz release's
    -- disambiguation; a Discogs release's company names, then its matrix /
    -- runout inscriptions.
    notes              TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(notes) AND json_type(notes) = 'array'),
    -- What a MusicBrainz release's documents state its album is on the other
    -- lookup catalog: 'read', the albums named in source_release_album_link
    -- (none, where nothing names one), or 'unread', where a document a
    -- statement reads was not fetched and none named an album. NULL for a
    -- Discogs release, whose documents state no counterpart album.
    album_links        TEXT CHECK (album_links IS NULL OR album_links IN ('read', 'unread')),
    -- The MusicBrainz release whose Cover Art Archive gallery the picker opens
    -- (this release, or the one a Discogs release cross-references), and its
    -- release group.
    archive_release_id TEXT,
    archive_group_id   TEXT,
    PRIMARY KEY (catalog, release_id),
    CHECK (archive_release_id IS NOT NULL OR archive_group_id IS NULL),
    CHECK (country IS NULL OR region IS NULL),
    CHECK ((catalog = 'musicbrainz') = (album_links IS NOT NULL))
) STRICT;

-- Every label the pressing is on, in its source's order, each stating a
-- name, a catalog number or both.
CREATE TABLE IF NOT EXISTS source_release_label (
    catalog        TEXT NOT NULL,
    release_id     TEXT NOT NULL,
    position       INTEGER NOT NULL CHECK (position >= 0),
    name           TEXT CHECK (name IS NULL OR name <> ''),
    catalog_number TEXT CHECK (catalog_number IS NULL OR catalog_number <> ''),
    -- The ids the catalogs know a named label by.
    musicbrainz_label_id TEXT CHECK (musicbrainz_label_id IS NULL OR musicbrainz_label_id <> ''),
    discogs_label_id     TEXT CHECK (discogs_label_id IS NULL OR discogs_label_id <> ''),
    CHECK (name IS NOT NULL OR (musicbrainz_label_id IS NULL AND discogs_label_id IS NULL)),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE,
    CHECK (name IS NOT NULL OR catalog_number IS NOT NULL)
) STRICT;

-- Every barcode the record states, in its order.
CREATE TABLE IF NOT EXISTS source_release_barcode (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    barcode    TEXT NOT NULL CHECK (barcode <> ''),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- The album's artists, in credit order.
CREATE TABLE IF NOT EXISTS source_release_album_artist (
    catalog               TEXT NOT NULL,
    release_id            TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    -- The name the catalog prints for the credit, where it is not `name`.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    -- What the catalog prints after the credit when another follows.
    join_phrase           TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- The releases on other catalogs a MusicBrainz release names as the same
-- release, in relation order.
CREATE TABLE IF NOT EXISTS source_release_link (
    catalog      TEXT NOT NULL CHECK (catalog = 'musicbrainz'),
    release_id   TEXT NOT NULL,
    position     INTEGER NOT NULL CHECK (position >= 0),
    link_catalog TEXT NOT NULL CHECK (link_catalog <> ''),
    link_key     TEXT NOT NULL CHECK (link_key <> ''),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- Each album on another catalog a MusicBrainz release's documents name as its
-- album, for a release whose album_links is 'read', in the order the
-- statements were read, strongest first: its release group's page or its own
-- links it; a Wikidata item that page links states it; or the release links a
-- twin release whose own document files it under the album. Which statement
-- named it is not kept.
CREATE TABLE IF NOT EXISTS source_release_album_link (
    catalog             TEXT NOT NULL CHECK (catalog = 'musicbrainz'),
    release_id          TEXT NOT NULL,
    ordinal             INTEGER NOT NULL CHECK (ordinal >= 0),
    album_catalog       TEXT NOT NULL CHECK (album_catalog <> '' AND album_catalog <> 'musicbrainz'),
    album_key           TEXT NOT NULL CHECK (album_key <> ''),
    PRIMARY KEY (catalog, release_id, ordinal),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- A Discogs release's format entries that are media, in order: the carrier (a
-- `crate::pressing::Medium` key, NULL where bae does not know it) and the
-- stated quantity.
CREATE TABLE IF NOT EXISTS source_release_format (
    catalog    TEXT NOT NULL CHECK (catalog = 'discogs'),
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    medium     TEXT CHECK (medium IS NULL OR medium <> ''),
    quantity   INTEGER NOT NULL CHECK (quantity >= 1),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- What another catalog says this release is ('pressing', with the album it
-- files it under) or what its album is ('album'). The release's own catalog is
-- the source_release row itself.
CREATE TABLE IF NOT EXISTS source_release_record (
    catalog        TEXT NOT NULL,
    release_id     TEXT NOT NULL,
    record_catalog TEXT NOT NULL CHECK (record_catalog <> ''),
    kind           TEXT NOT NULL CHECK (kind IN ('pressing', 'album')),
    key            TEXT NOT NULL CHECK (key <> ''),
    album_key      TEXT,
    PRIMARY KEY (catalog, release_id, record_catalog),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE,
    CHECK (record_catalog <> catalog),
    CHECK (kind = 'pressing' OR album_key IS NULL)
) STRICT;

-- The images the release offers a picker: its pressing's own ('release'),
-- then its album's ('album'), each in the order they are offered.
CREATE TABLE IF NOT EXISTS source_release_cover (
    catalog       TEXT NOT NULL,
    release_id    TEXT NOT NULL,
    scope         TEXT NOT NULL CHECK (scope IN ('release', 'album')),
    position      INTEGER NOT NULL CHECK (position >= 0),
    -- The original; its downscaled copies are in source_release_cover_copy.
    url           TEXT NOT NULL,
    -- The cover's label as JSON (a `CoverLabel`).
    label         TEXT NOT NULL,
    source        TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    -- Whether a catalog says the image exists, or it is an address nothing
    -- said anything about.
    standing      TEXT NOT NULL CHECK (standing IN ('stated', 'unstated')),
    PRIMARY KEY (catalog, release_id, scope, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- The downscaled copies the catalog serves of one offered image, one per size
-- (a copy's longer side is at most max_edge pixels).
CREATE TABLE IF NOT EXISTS source_release_cover_copy (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    scope      TEXT NOT NULL,
    position   INTEGER NOT NULL,
    max_edge   INTEGER NOT NULL CHECK (max_edge > 0),
    url        TEXT NOT NULL CHECK (url <> ''),
    PRIMARY KEY (catalog, release_id, scope, position, max_edge),
    FOREIGN KEY (catalog, release_id, scope, position)
        REFERENCES source_release_cover (catalog, release_id, scope, position) ON DELETE CASCADE
) STRICT;

-- The MusicBrainz release groups the album was read from, whose Cover Art
-- Archive galleries the picker opens too.
CREATE TABLE IF NOT EXISTS source_release_archive_group (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    group_id   TEXT NOT NULL CHECK (group_id <> ''),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- The credits a release states for itself rather than for one track, at
-- their positions among its credits.
CREATE TABLE IF NOT EXISTS source_release_role (
    catalog               TEXT NOT NULL,
    release_id            TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    -- The name the catalog prints for the artist where it is another than the
    -- artist's own.
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    -- What the artist did: a `bae_types::credit_role::CreditRoleKind` key,
    -- 'performer' (its part in the catalog's words, if the catalog names one
    -- bae's lists lack, in `role_words`), 'instrument' (the id of an instrument on
    -- MusicBrainz's list in `role_words`), 'voice' (the id of a voice on MusicBrainz's list in `role_words`), or
    -- 'stated' for the catalog's own words (`role_words`), with the role's
    -- qualifiers as a JSON list.
    role                  TEXT NOT NULL CHECK (role IN ('writer', 'composer', 'lyricist', 'librettist', 'translator', 'reviser', 'scriptwriter', 'reconstructor', 'arranger', 'producer', 'engineer', 'mixer', 'mastering', 'performer', 'instrument', 'voice', 'stated')),
    role_words            TEXT CHECK (role_words IS NULL OR role_words <> ''),
    qualifiers            TEXT NOT NULL CHECK (json_valid(qualifiers) AND json_type(qualifiers) = 'array'),
    CHECK (CASE role WHEN 'stated' THEN role_words IS NOT NULL WHEN 'instrument' THEN role_words IS NOT NULL WHEN 'voice' THEN role_words IS NOT NULL WHEN 'performer' THEN 1 ELSE role_words IS NULL END),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- Every medium of the release, in order, with its carrier (a
-- `crate::pressing::Medium` key, NULL where bae does not know it): the format
-- a MusicBrainz medium states, or for a Discogs release, whose one tracklist
-- is split into the mediums its positions name, the carrier a position prefix
-- or the release's format entries give each.
CREATE TABLE IF NOT EXISTS source_release_medium (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 0),
    medium     TEXT CHECK (medium IS NULL OR medium <> ''),
    PRIMARY KEY (catalog, release_id, position),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- The disc IDs MusicBrainz has registered against a medium, in its order: a
-- folder whose rip hashes to one of them is a rip of that medium. A Discogs
-- medium holds none.
CREATE TABLE IF NOT EXISTS source_release_medium_disc (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    position   INTEGER NOT NULL,
    disc       INTEGER NOT NULL CHECK (disc >= 0),
    disc_id    TEXT NOT NULL CHECK (disc_id <> ''),
    PRIMARY KEY (catalog, release_id, position, disc),
    UNIQUE (catalog, release_id, position, disc_id),
    FOREIGN KEY (catalog, release_id, position)
        REFERENCES source_release_medium (catalog, release_id, position) ON DELETE CASCADE
) STRICT;

-- The linked documents a fetch did not get, whose facts the release's rows
-- lack: 'failed' when the source was asked and failed,
-- 'not_asked' for one on a catalog the read did not ask (switched off, or
-- Discogs with no key to ask with),
-- 'stopped_short' for one an identification read does not follow, or the
-- release's own document where the read took the release from a Disc ID
-- lookup's answer, which a complete read fetches. A new fetch replaces them.
CREATE TABLE IF NOT EXISTS source_release_unfetched (
    catalog    TEXT NOT NULL,
    release_id TEXT NOT NULL,
    document   TEXT NOT NULL,
    key        TEXT NOT NULL CHECK (key <> ''),
    reason     TEXT NOT NULL CHECK (reason IN ('failed', 'not_asked', 'stopped_short')),
    PRIMARY KEY (catalog, release_id, document, key),
    FOREIGN KEY (catalog, release_id)
        REFERENCES source_release (catalog, release_id) ON DELETE CASCADE
) STRICT;

-- A row that stops naming a catalog release, deleted or renamed, deletes the
-- release when no other row names it.
CREATE TRIGGER IF NOT EXISTS source_release_unnamed_by_match
AFTER DELETE ON import_candidate_match
BEGIN
    DELETE FROM source_release
    WHERE catalog = OLD.source AND release_id = OLD.release_id
      AND NOT EXISTS (SELECT 1 FROM import_candidate_match
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_draft_provenance
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link_partner
                      WHERE source = OLD.source AND release_id = OLD.release_id);
END;

CREATE TRIGGER IF NOT EXISTS source_release_renamed_by_match
AFTER UPDATE OF source, release_id ON import_candidate_match
WHEN OLD.source IS NOT NEW.source OR OLD.release_id IS NOT NEW.release_id
BEGIN
    DELETE FROM source_release
    WHERE catalog = OLD.source AND release_id = OLD.release_id
      AND NOT EXISTS (SELECT 1 FROM import_candidate_match
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_draft_provenance
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link_partner
                      WHERE source = OLD.source AND release_id = OLD.release_id);
END;

CREATE TRIGGER IF NOT EXISTS source_release_unnamed_by_provenance
AFTER DELETE ON import_candidate_draft_provenance
BEGIN
    DELETE FROM source_release
    WHERE catalog = OLD.source AND release_id = OLD.release_id
      AND NOT EXISTS (SELECT 1 FROM import_candidate_match
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_draft_provenance
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link_partner
                      WHERE source = OLD.source AND release_id = OLD.release_id);
END;

CREATE TRIGGER IF NOT EXISTS source_release_renamed_by_provenance
AFTER UPDATE OF source, release_id ON import_candidate_draft_provenance
WHEN OLD.source IS NOT NEW.source OR OLD.release_id IS NOT NEW.release_id
BEGIN
    DELETE FROM source_release
    WHERE catalog = OLD.source AND release_id = OLD.release_id
      AND NOT EXISTS (SELECT 1 FROM import_candidate_match
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_draft_provenance
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link_partner
                      WHERE source = OLD.source AND release_id = OLD.release_id);
END;

CREATE TRIGGER IF NOT EXISTS source_release_unnamed_by_link
AFTER DELETE ON import_candidate_release_link
BEGIN
    DELETE FROM source_release
    WHERE catalog = OLD.source AND release_id = OLD.release_id
      AND NOT EXISTS (SELECT 1 FROM import_candidate_match
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_draft_provenance
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link_partner
                      WHERE source = OLD.source AND release_id = OLD.release_id);
END;

CREATE TRIGGER IF NOT EXISTS source_release_renamed_by_link
AFTER UPDATE OF source, release_id ON import_candidate_release_link
WHEN OLD.source IS NOT NEW.source OR OLD.release_id IS NOT NEW.release_id
BEGIN
    DELETE FROM source_release
    WHERE catalog = OLD.source AND release_id = OLD.release_id
      AND NOT EXISTS (SELECT 1 FROM import_candidate_match
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_draft_provenance
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link_partner
                      WHERE source = OLD.source AND release_id = OLD.release_id);
END;

CREATE TRIGGER IF NOT EXISTS source_release_unnamed_by_link_partner
AFTER DELETE ON import_candidate_release_link_partner
BEGIN
    DELETE FROM source_release
    WHERE catalog = OLD.source AND release_id = OLD.release_id
      AND NOT EXISTS (SELECT 1 FROM import_candidate_match
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_draft_provenance
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link
                      WHERE source = OLD.source AND release_id = OLD.release_id)
      AND NOT EXISTS (SELECT 1 FROM import_candidate_release_link_partner
                      WHERE source = OLD.source AND release_id = OLD.release_id);
END;
