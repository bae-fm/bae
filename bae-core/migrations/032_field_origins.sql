-- Where each album-level value came from, per field rather than per draft.
--
-- A draft used to state one provenance for the whole of it, so a value a
-- person typed and a value a mapper wrote were the same kind of thing. Each
-- field now carries its own origin beside its value: `record:<catalog>` for a
-- catalog's description, `tags` for the files' own tags, `typed` for a value a
-- person entered. A blank field carries none — an absent value came from
-- nowhere.
--
-- Both sides get the same eight columns: the candidate's draft, and the
-- release the edit sheet edits. What every catalog says about a field is not
-- stored at all; it is read back from the archived documents.

ALTER TABLE import_candidate_edit ADD COLUMN album_title_origin TEXT;
ALTER TABLE import_candidate_edit ADD COLUMN album_year_origin TEXT;
ALTER TABLE import_candidate_edit ADD COLUMN year_origin TEXT;
ALTER TABLE import_candidate_edit ADD COLUMN format_origin TEXT;
ALTER TABLE import_candidate_edit ADD COLUMN label_origin TEXT;
ALTER TABLE import_candidate_edit ADD COLUMN catalog_number_origin TEXT;
ALTER TABLE import_candidate_edit ADD COLUMN country_origin TEXT;
ALTER TABLE import_candidate_edit ADD COLUMN barcode_origin TEXT;

ALTER TABLE releases ADD COLUMN album_title_origin TEXT;
ALTER TABLE releases ADD COLUMN album_year_origin TEXT;
ALTER TABLE releases ADD COLUMN year_origin TEXT;
ALTER TABLE releases ADD COLUMN format_origin TEXT;
ALTER TABLE releases ADD COLUMN label_origin TEXT;
ALTER TABLE releases ADD COLUMN catalog_number_origin TEXT;
ALTER TABLE releases ADD COLUMN country_origin TEXT;
ALTER TABLE releases ADD COLUMN barcode_origin TEXT;

-- The one origin a stored draft can be backfilled to: whatever filled it whole.
-- A candidate nobody has picked a source for has no origin for any field, and a
-- typed edit is indistinguishable from the mapper write beside it, which is the
-- conflation this migration ends going forward.
CREATE TEMP TABLE candidate_draft_origin AS
SELECT
    content_hash,
    CASE kind
        WHEN 'file_tags' THEN 'tags'
        WHEN 'external_release' THEN 'record:' || source
    END AS origin
FROM import_candidate_draft_provenance;

UPDATE import_candidate_edit SET
    album_title_origin = (
        SELECT CASE WHEN TRIM(import_candidate_edit.album_title) <> '' THEN o.origin END
        FROM candidate_draft_origin o WHERE o.content_hash = import_candidate_edit.content_hash),
    album_year_origin = (
        SELECT CASE WHEN TRIM(import_candidate_edit.album_year) <> '' THEN o.origin END
        FROM candidate_draft_origin o WHERE o.content_hash = import_candidate_edit.content_hash),
    year_origin = (
        SELECT CASE WHEN TRIM(import_candidate_edit.year) <> '' THEN o.origin END
        FROM candidate_draft_origin o WHERE o.content_hash = import_candidate_edit.content_hash),
    format_origin = (
        SELECT CASE WHEN TRIM(import_candidate_edit.format) <> '' THEN o.origin END
        FROM candidate_draft_origin o WHERE o.content_hash = import_candidate_edit.content_hash),
    label_origin = (
        SELECT CASE WHEN TRIM(import_candidate_edit.label) <> '' THEN o.origin END
        FROM candidate_draft_origin o WHERE o.content_hash = import_candidate_edit.content_hash),
    catalog_number_origin = (
        SELECT CASE WHEN TRIM(import_candidate_edit.catalog_number) <> '' THEN o.origin END
        FROM candidate_draft_origin o WHERE o.content_hash = import_candidate_edit.content_hash),
    country_origin = (
        SELECT CASE WHEN TRIM(import_candidate_edit.country) <> '' THEN o.origin END
        FROM candidate_draft_origin o WHERE o.content_hash = import_candidate_edit.content_hash),
    barcode_origin = (
        SELECT CASE WHEN TRIM(import_candidate_edit.barcode) <> '' THEN o.origin END
        FROM candidate_draft_origin o WHERE o.content_hash = import_candidate_edit.content_hash);

DROP TABLE candidate_draft_origin;

-- The release's own answer: the record that reads its draft, or the files'
-- tags. A release that started blank was read from nothing.
CREATE TEMP TABLE release_draft_origin AS
SELECT
    r.id AS release_id,
    CASE
        WHEN r.draft_from_tags = 1 THEN 'tags'
        ELSE (
            SELECT 'record:' || rec.catalog FROM release_records rec
            WHERE rec.release_id = r.id AND rec.reads_draft = 1)
    END AS origin,
    (SELECT a.title FROM albums a WHERE a.id = r.album_id) AS album_title,
    (SELECT a.year FROM albums a WHERE a.id = r.album_id) AS album_year
FROM releases r;

UPDATE releases SET
    album_title_origin = (
        SELECT CASE WHEN TRIM(COALESCE(o.album_title, '')) <> '' THEN o.origin END
        FROM release_draft_origin o WHERE o.release_id = releases.id),
    album_year_origin = (
        SELECT CASE WHEN o.album_year IS NOT NULL THEN o.origin END
        FROM release_draft_origin o WHERE o.release_id = releases.id),
    year_origin = (
        SELECT CASE WHEN releases.year IS NOT NULL THEN o.origin END
        FROM release_draft_origin o WHERE o.release_id = releases.id),
    format_origin = (
        SELECT CASE WHEN TRIM(COALESCE(releases.format, '')) <> '' THEN o.origin END
        FROM release_draft_origin o WHERE o.release_id = releases.id),
    label_origin = (
        SELECT CASE WHEN TRIM(COALESCE(releases.label, '')) <> '' THEN o.origin END
        FROM release_draft_origin o WHERE o.release_id = releases.id),
    catalog_number_origin = (
        SELECT CASE WHEN TRIM(COALESCE(releases.catalog_number, '')) <> '' THEN o.origin END
        FROM release_draft_origin o WHERE o.release_id = releases.id),
    country_origin = (
        SELECT CASE WHEN TRIM(COALESCE(releases.country, '')) <> '' THEN o.origin END
        FROM release_draft_origin o WHERE o.release_id = releases.id),
    barcode_origin = (
        SELECT CASE WHEN TRIM(COALESCE(releases.barcode, '')) <> '' THEN o.origin END
        FROM release_draft_origin o WHERE o.release_id = releases.id);

DROP TABLE release_draft_origin;
