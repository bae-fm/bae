-- The mapping pane's # column now reads the draft track's own numbering, so a
-- stored row no longer carries the picked source's position string. What the
-- row keeps is the fact that string stood in for: whether the source's
-- tracklist contains the track. A stored position proves it did; its absence
-- marks a row that exists only because audio was found for it.
ALTER TABLE import_candidate_track_mapping ADD COLUMN named_by_source INTEGER NOT NULL DEFAULT 0;
UPDATE import_candidate_track_mapping SET named_by_source = source_position IS NOT NULL;
ALTER TABLE import_candidate_track_mapping DROP COLUMN source_position;
