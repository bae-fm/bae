-- Preserve the existing library order before allowing unknown side assignments.
ALTER TABLE tracks ADD COLUMN position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0);
WITH ordered AS (
    SELECT id, ROW_NUMBER() OVER (
        PARTITION BY release_id ORDER BY side, track_number, id
    ) - 1 AS position FROM tracks
)
UPDATE tracks SET position = (SELECT position FROM ordered WHERE ordered.id = tracks.id);
CREATE UNIQUE INDEX tracks_release_position ON tracks(release_id, position);

ALTER TABLE tracks ADD COLUMN assigned_side INTEGER;
UPDATE tracks SET assigned_side = side;
ALTER TABLE tracks DROP COLUMN side;
ALTER TABLE tracks RENAME COLUMN assigned_side TO side;
