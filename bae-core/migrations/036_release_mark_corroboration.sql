-- A reading is not evidence that its lookup found the chosen record.
-- Existing releases recorded no per-value proof, so their readings claim none.
ALTER TABLE release_marks ADD COLUMN corroborated INTEGER NOT NULL DEFAULT 0
    CHECK (corroborated IN (0, 1));
