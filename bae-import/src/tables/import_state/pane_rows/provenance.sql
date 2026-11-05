-- Where the draft was read from; who wrote it is `import_candidate_edit.author`.
CREATE TABLE IF NOT EXISTS import_candidate_draft_provenance (
    content_hash TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('external_release', 'file_tags')),
    source       TEXT CHECK (source IS NULL OR source IN ('musicbrainz', 'discogs')),
    release_id   TEXT,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_edit (content_hash) ON DELETE CASCADE,
    -- The release is stored in the write that names it here, in either order,
    -- so the key is checked as the write commits.
    FOREIGN KEY (source, release_id) REFERENCES source_release (catalog, release_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK ((kind = 'external_release') = (source IS NOT NULL)),
    CHECK ((kind = 'external_release') = (release_id IS NOT NULL))
) STRICT;

-- The drafts read from a catalog release, which keep it stored.
CREATE INDEX IF NOT EXISTS idx_import_candidate_draft_provenance_release
    ON import_candidate_draft_provenance (source, release_id);
