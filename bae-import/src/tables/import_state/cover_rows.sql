-- The cover the import will write: a file of the folder, a tag's embedded
-- image, or one offered by a catalog.
CREATE TABLE IF NOT EXISTS import_candidate_cover (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('local', 'remote', 'embedded')),
    file_id      TEXT,
    url          TEXT,
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    CHECK ((kind IN ('local', 'embedded')) = (file_id IS NOT NULL)),
    -- Each on its own: a folder's or a tag's cover holds neither half.
    CHECK ((kind = 'remote') = (url IS NOT NULL)),
    CHECK ((kind = 'remote') = (source IS NOT NULL)),
    -- Referenced by import_candidate_cover_copy; only a remote cover has a url.
    UNIQUE (content_hash, url)
) STRICT;

-- The downscaled copies the catalog serves of a remote cover, one per size (a
-- copy's longer side is at most max_edge pixels). A local or embedded cover
-- has no url, so it has none.
CREATE TABLE IF NOT EXISTS import_candidate_cover_copy (
    content_hash TEXT NOT NULL,
    image_url    TEXT NOT NULL,
    max_edge     INTEGER NOT NULL CHECK (max_edge > 0),
    url          TEXT NOT NULL CHECK (url <> ''),
    PRIMARY KEY (content_hash, max_edge),
    FOREIGN KEY (content_hash, image_url)
        REFERENCES import_candidate_cover (content_hash, url) ON DELETE CASCADE
) STRICT;

-- The bytes of a catalog-offered cover, fetched before the import runs.
CREATE TABLE IF NOT EXISTS import_candidate_remote_cover_asset (
    content_hash TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    bytes        BLOB NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_cover (content_hash) ON DELETE CASCADE
) STRICT;
