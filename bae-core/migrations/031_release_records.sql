-- A release is bae's own row; every catalog that describes it is a record:
-- which catalog, its key for this release, the group that key belongs to
-- there, the page the catalog publishes, and whether the draft's facts were
-- read from it.
--
-- `release_identities` held at most two rows per release and each had to name
-- one of the two catalogs bae asks. Its rows become records: the catalog and
-- key it already held, the page built from them, and `reads_draft` for the one
-- the release's own column pointed at.
--
-- What the release itself still says about its draft is only whether it came
-- off the files' own tags. The record carrying `reads_draft` names the
-- document otherwise, and a release with neither started blank, so
-- `metadata_source` and `metadata_source_release_id` go and
-- `draft_from_tags` takes their place.
CREATE TABLE release_records (
    id          TEXT NOT NULL PRIMARY KEY,
    release_id  TEXT NOT NULL,
    catalog     TEXT NOT NULL,
    key         TEXT NOT NULL,
    group_key   TEXT NOT NULL,
    url         TEXT NOT NULL CHECK (url <> ''),
    reads_draft INTEGER NOT NULL DEFAULT 0 CHECK (reads_draft IN (0, 1)),
    _updated_at TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    UNIQUE (release_id, catalog),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

-- Import dedup asks both questions of a record: is this exact pressing already
-- in the library, and is anything from the group it belongs to.
CREATE INDEX idx_release_records_catalog_key ON release_records (catalog, key);
CREATE INDEX idx_release_records_catalog_group
    ON release_records (catalog, group_key);

-- One record per release reads the draft, never two.
CREATE UNIQUE INDEX idx_release_records_reads_draft
    ON release_records (release_id) WHERE reads_draft = 1;

INSERT INTO release_records (
    id, release_id, catalog, key, group_key, url, reads_draft,
    _updated_at, created_at
)
SELECT
    i.id,
    i.release_id,
    i.source,
    i.source_release_id,
    i.source_group_id,
    -- The page each of the two catalogs publishes releases at — what
    -- `Catalog::release_url` builds.
    CASE i.source
        WHEN 'musicbrainz'
            THEN 'https://musicbrainz.org/release/' || i.source_release_id
        WHEN 'discogs'
            THEN 'https://www.discogs.com/release/' || i.source_release_id
    END,
    -- The release used to point at the document its draft was read from. That
    -- is a fact about the records: the one whose catalog the release named is
    -- the one the draft reads.
    CASE WHEN r.metadata_source = i.source THEN 1 ELSE 0 END,
    i._updated_at,
    i.created_at
FROM release_identities i
JOIN releases r ON r.id = i.release_id;

DROP TABLE release_identities;

DROP TRIGGER releases_metadata_provenance_insert;
DROP TRIGGER releases_metadata_provenance_update;

ALTER TABLE releases ADD COLUMN draft_from_tags INTEGER NOT NULL DEFAULT 0
    CHECK (draft_from_tags IN (0, 1));

UPDATE releases SET draft_from_tags = 1 WHERE metadata_source = 'file_tags';

ALTER TABLE releases DROP COLUMN metadata_source_release_id;
ALTER TABLE releases DROP COLUMN metadata_source;
