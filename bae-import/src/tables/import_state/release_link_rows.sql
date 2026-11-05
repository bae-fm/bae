-- What the candidate is linked to in the catalogs: the pressing picked for its
-- folder ('pressing', naming the release the pick names first), or the album
-- the person could not tell the pressing of ('album', its albums in
-- import_candidate_release_link_album). Independent of the draft: only a
-- pick, identification applying its pick, linking the album, keeping the
-- folder's own draft, or unlinking changes it.
CREATE TABLE IF NOT EXISTS import_candidate_release_link (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('pressing', 'album')),
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    release_id   TEXT,
    -- What the partner and album rows name, so each hangs off a link of its
    -- own kind only.
    UNIQUE (content_hash, kind),
    FOREIGN KEY (content_hash)
        REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE,
    -- Checked as the write commits, as the provenance's is.
    FOREIGN KEY (source, release_id) REFERENCES source_release (catalog, release_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK ((kind = 'pressing') = (source IS NOT NULL)),
    CHECK ((kind = 'pressing') = (release_id IS NOT NULL))
) STRICT;

-- The candidates linked to a catalog release, which keep it stored.
CREATE INDEX IF NOT EXISTS idx_import_candidate_release_link_release
    ON import_candidate_release_link (source, release_id);

-- The releases in the other catalogs the linked pressing paired with.
CREATE TABLE IF NOT EXISTS import_candidate_release_link_partner (
    content_hash TEXT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind = 'pressing'),
    source       TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    release_id   TEXT NOT NULL,
    PRIMARY KEY (content_hash, source),
    FOREIGN KEY (content_hash, kind)
        REFERENCES import_candidate_release_link (content_hash, kind) ON DELETE CASCADE,
    -- Checked as the write commits, as the provenance's is.
    FOREIGN KEY (source, release_id) REFERENCES source_release (catalog, release_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

-- The linked pressings a catalog release partners, which keep it stored.
CREATE INDEX IF NOT EXISTS idx_import_candidate_release_link_partner_release
    ON import_candidate_release_link_partner (source, release_id);

-- The linked album in each catalog that files it: its MusicBrainz release
-- group, its Discogs master.
CREATE TABLE IF NOT EXISTS import_candidate_release_link_album (
    content_hash TEXT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind = 'album'),
    catalog      TEXT NOT NULL CHECK (catalog IN ('musicbrainz', 'discogs')),
    album_id     TEXT NOT NULL CHECK (album_id <> ''),
    PRIMARY KEY (content_hash, catalog),
    FOREIGN KEY (content_hash, kind)
        REFERENCES import_candidate_release_link (content_hash, kind) ON DELETE CASCADE
) STRICT;
