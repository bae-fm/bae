-- What a run showed while it ran is what the pane shows after it saved.
--
-- The ledger — one row per value extraction found, with where it was found
-- beside it and one cell per provider asked about it — used to be rebuilt on
-- read from the verdict's matches and the candidate's stored signals. The
-- rebuild could only name providers the matches name, so a provider that was
-- asked and whose every answer the agreement narrowed out lost its column,
-- along with per-code walk ends, per-provider counts, and the failures of a
-- run that ended found.
--
-- The run records its ledger once, as its last frame showed it, and it is
-- stored here beside the verdict it settled on. NULL is "no ledger recorded":
-- a verdict written before this, and a run extraction handed nothing to lay
-- out. Both draw the settled lists with no ledger beside them.
ALTER TABLE import_candidate_verdict
    ADD COLUMN ledger_json TEXT CHECK (
        ledger_json IS NULL
        OR (json_valid(ledger_json) AND json_type(ledger_json) = 'object')
    );
