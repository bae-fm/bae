-- Stored scan rows used byte digests that are no longer part of candidate identity.
-- Remove the cached scans so every watched folder is scanned into the new shape.
DELETE FROM folder_scan_roots;

ALTER TABLE scan_candidate_file DROP COLUMN content_digest;
