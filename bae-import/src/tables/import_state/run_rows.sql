-- An identify run in flight: the key it identifies. How it stands is its
-- import_candidate_verdict row, its matches and its ledger rows, and what
-- extraction read for it is its import_candidate_signals row, each owned by
-- `identify_run`, so a run in flight is stored in the shape of the verdict it
-- may end on. A folder candidate's run has its row only while the
-- candidate's standing names the run: its first state is written in the
-- write that starts it, a later one only while the standing still names it,
-- and the triggers below delete it in the write that moves the standing off
-- it. A library release's re-identify run, which has no standing, keeps its
-- row until it ends or its sheet closes. No run outlives the process, so the
-- table is emptied as the library opens, with the standings.
CREATE TABLE IF NOT EXISTS import_identify_run (
    identify_run  INTEGER PRIMARY KEY CHECK (identify_run >= 0),
    candidate_key BLOB NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_import_identify_run_key ON import_identify_run (candidate_key);

CREATE TRIGGER IF NOT EXISTS import_identify_run_on_standing_moved
AFTER UPDATE OF candidate_key, identify_run ON import_candidate_standing
WHEN (OLD.candidate_key, OLD.identify_run) IS NOT (NEW.candidate_key, NEW.identify_run)
BEGIN
    DELETE FROM import_identify_run
    WHERE candidate_key = OLD.candidate_key
      AND (candidate_key, identify_run) IS NOT (NEW.candidate_key, NEW.identify_run);
END;

CREATE TRIGGER IF NOT EXISTS import_identify_run_on_standing_removed
AFTER DELETE ON import_candidate_standing
BEGIN
    DELETE FROM import_identify_run WHERE candidate_key = OLD.candidate_key;
END;
