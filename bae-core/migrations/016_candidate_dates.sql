-- NULL means a candidate predates date tracking and has not been observed
-- since. Do not invent a historical first-discovery time for those rows.
ALTER TABLE scan_candidate ADD COLUMN first_seen_at INTEGER;
ALTER TABLE scan_candidate ADD COLUMN source_date INTEGER;
ALTER TABLE scan_candidate ADD COLUMN source_date_kind TEXT
    CHECK ((source_date IS NULL AND source_date_kind IS NULL)
        OR (source_date IS NOT NULL AND source_date_kind IS NOT NULL
            AND source_date_kind IN ('added_to_directory', 'created')));

-- A cached directory mtime cannot answer whether folder dates were captured.
-- Preserve the candidates while requiring a complete observation of each root.
DELETE FROM folder_scan_directory;
