-- Which name read off the object tied its files to the record the release's
-- draft was read from: the disc ID derived from the rip log's table of
-- contents, a barcode read off a scan, or a catalog number read off the folder.
--
-- The lookups already record this per match while a folder is a candidate
-- (`import_candidate_match.by_disc_id` / `by_barcode` / `by_catalog`), and it
-- died at commit like the marks did. NULL where nothing tied them: a release
-- whose record somebody picked from search, one read off the files' own tags,
-- and one that started blank.
--
-- Not the same question as which marks the release carries. A folder can state
-- a barcode no catalog answered to, and a mark row says only that the object
-- carries the name.
ALTER TABLE releases ADD COLUMN identified_by TEXT
    CHECK (identified_by IS NULL
           OR identified_by IN ('disc_id', 'barcode', 'catalog_number'));
