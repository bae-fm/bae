-- coven's blob declarations now require a content-hash column on every
-- blob-bearing table (`BlobDecl::hash_column`, defaulting to `hash`): the
-- lowercase-hex SHA-256 of a blob's plaintext, signed on the row alongside its
-- declared size, verified against the decrypted bytes on a Remote fetch. Add
-- the column to the three blob-bearing tables so `BlobDecls::from_tables`
-- resolves it — without it, coven refuses every write and pull touching these
-- tables (`BlobDeclError::MissingColumn`).
--
-- Nullable: no import-time population lands in this migration (see the
-- content-hash rollout note in bae's coven-bump notes), so an existing row's
-- hash is NULL until a follow-up populates it. A NULL hash only ever surfaces
-- as a failure (`BlobCacheError::MissingContentHash`) on a Remote fetch of
-- that specific blob — every local-only read and write is unaffected.
ALTER TABLE release_files ADD COLUMN hash TEXT;
ALTER TABLE covers ADD COLUMN hash TEXT;
ALTER TABLE artist_images ADD COLUMN hash TEXT;
