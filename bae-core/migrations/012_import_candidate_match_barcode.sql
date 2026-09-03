-- A stored match now keeps the barcode its source printed, which is what pairs
-- a MusicBrainz row and a Discogs row into one pressing.
ALTER TABLE import_candidate_match ADD COLUMN barcode TEXT;

-- A failed verdict's `failures_json` now names the provider that failed, not
-- just the lookup. These rows are device-local derived state: the queue sweep
-- re-derives one for any candidate that still needs it, so dropping them costs
-- a re-run rather than an answer.
DELETE FROM import_candidate_identify_failure;
