-- The candidates the user dismissed, by folder path (coven's bytes for it),
-- so a later scan does not offer them again.
CREATE TABLE IF NOT EXISTS skipped_import_candidates (
    candidate_path BLOB PRIMARY KEY
) STRICT;
