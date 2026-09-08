-- Agreement between a candidate's signals is what makes a match list short: a
-- disc ID that named three releases and a barcode that named two settle on the
-- one they share, and the other four never reach the person. Each of those four
-- is a real answer from a real lookup, and one of them may be the disc on the
-- desk, so a verdict now keeps them beside its matches — same columns, same
-- position sequence, marked as what agreement left out.
--
-- Every row written before this is a match: the verdicts that stored them kept
-- nothing else.
ALTER TABLE import_candidate_match
    ADD COLUMN narrowed_out INTEGER NOT NULL DEFAULT 0 CHECK (narrowed_out IN (0, 1));
