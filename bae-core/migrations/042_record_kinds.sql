-- Catalog album pages describe no particular pressing. A pressing's parent is
-- optional, and equality between its key and the former group key is ambiguous.
-- Keep existing column ordinals: the sync routing contract includes the clock's
-- ordinal. New columns follow the columns whose positions are already pinned.
CREATE TABLE release_records_with_kind (
    id          TEXT NOT NULL PRIMARY KEY,
    release_id  TEXT NOT NULL,
    catalog     TEXT NOT NULL,
    key         TEXT NOT NULL,
    album_key   TEXT,
    url         TEXT NOT NULL CHECK (url <> ''),
    reads_draft INTEGER NOT NULL CHECK (reads_draft IN (0, 1)),
    _updated_at TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('pressing', 'album')),
    CHECK (kind = 'pressing' OR (album_key IS NULL AND reads_draft = 0)),
    UNIQUE (release_id, catalog),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
) STRICT;

INSERT INTO release_records_with_kind
    (id, release_id, catalog, kind, key, album_key, url, reads_draft, _updated_at, created_at)
SELECT rr.id, rr.release_id, rr.catalog,
    CASE WHEN rr.catalog IN ('musicbrainz', 'discogs') THEN 'pressing' ELSE 'album' END,
    rr.key,
    CASE
        WHEN rr.catalog NOT IN ('musicbrainz', 'discogs') THEN NULL
        -- Only a distinct canonical key established a parent. Cached documents
        -- are local and cannot determine the result of a synced migration.
        WHEN rr.group_key <> rr.key THEN rr.group_key
        ELSE NULL
    END,
    rr.url,
    CASE WHEN rr.catalog IN ('musicbrainz', 'discogs') THEN rr.reads_draft ELSE 0 END,
    rr._updated_at, rr.created_at
FROM release_records rr;

DROP TABLE release_records;
ALTER TABLE release_records_with_kind RENAME TO release_records;
CREATE INDEX idx_release_records_catalog_key ON release_records (catalog, key) WHERE kind = 'pressing';
CREATE INDEX idx_release_records_catalog_album ON release_records
    (catalog, CASE WHEN kind = 'album' THEN key ELSE album_key END);
CREATE UNIQUE INDEX idx_release_records_reads_draft
    ON release_records (release_id) WHERE reads_draft = 1;
