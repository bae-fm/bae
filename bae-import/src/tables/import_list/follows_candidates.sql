-- The placement follows each release's stored import state: each change to
-- its state, draft, album credits, provenance, link, verdict and found
-- releases, failure and what is running for it places again the releases it
-- can move (`import_list_replace`). A change to a column the rules don't read
-- places nothing.

-- Storing or removing a state places nothing by itself: everything the rules
-- read of it hangs off it by foreign key, written after it and deleted with
-- it, and those rows' own triggers place the release. Moving the revision it
-- describes does.
CREATE TRIGGER IF NOT EXISTS import_list_on_state_changed
AFTER UPDATE OF content_hash, edit_revision ON import_candidate_state
WHEN (OLD.content_hash, OLD.edit_revision) IS NOT (NEW.content_hash, NEW.edit_revision)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_draft_added AFTER INSERT ON import_candidate_edit
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_draft_removed AFTER DELETE ON import_candidate_edit
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_draft_changed
AFTER UPDATE OF content_hash, album_title, draft_blank, draft_valid ON import_candidate_edit
WHEN (OLD.content_hash, OLD.album_title, OLD.draft_blank, OLD.draft_valid)
  IS NOT (NEW.content_hash, NEW.album_title, NEW.draft_blank, NEW.draft_valid)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_album_artist_added AFTER INSERT ON import_candidate_album_artist_assignment
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_album_artist_removed AFTER DELETE ON import_candidate_album_artist_assignment
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_album_artist_changed
AFTER UPDATE OF content_hash, position, assignment_kind, artist_id, name, credit_key, credited_as, join_phrase ON import_candidate_album_artist_assignment
WHEN (OLD.content_hash, OLD.position, OLD.assignment_kind, OLD.artist_id, OLD.name, OLD.credit_key, OLD.credited_as, OLD.join_phrase) IS NOT (NEW.content_hash, NEW.position, NEW.assignment_kind, NEW.artist_id, NEW.name, NEW.credit_key, NEW.credited_as, NEW.join_phrase)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_provenance_added AFTER INSERT ON import_candidate_draft_provenance
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_provenance_removed AFTER DELETE ON import_candidate_draft_provenance
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_provenance_changed
AFTER UPDATE OF content_hash ON import_candidate_draft_provenance
WHEN OLD.content_hash IS NOT NEW.content_hash
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_link_added AFTER INSERT ON import_candidate_release_link
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_link_removed AFTER DELETE ON import_candidate_release_link
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_link_changed
AFTER UPDATE OF content_hash ON import_candidate_release_link
WHEN OLD.content_hash IS NOT NEW.content_hash
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_verdict_added AFTER INSERT ON import_candidate_verdict
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_verdict_removed AFTER DELETE ON import_candidate_verdict
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_verdict_changed
AFTER UPDATE OF content_hash, kind, track_count, medium_conflict, error_detail, kept_own_draft
ON import_candidate_verdict
WHEN (OLD.content_hash, OLD.kind, OLD.track_count, OLD.medium_conflict, OLD.error_detail,
      OLD.kept_own_draft)
  IS NOT (NEW.content_hash, NEW.kind, NEW.track_count, NEW.medium_conflict, NEW.error_detail,
          NEW.kept_own_draft)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_match_added AFTER INSERT ON import_candidate_match
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_match_removed AFTER DELETE ON import_candidate_match
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_match_changed
AFTER UPDATE OF content_hash, position, pressing, listing, source_tracks_kind, source_tracks_count, document_failure ON import_candidate_match
WHEN (OLD.content_hash, OLD.position, OLD.pressing, OLD.listing, OLD.source_tracks_kind, OLD.source_tracks_count, OLD.document_failure) IS NOT (NEW.content_hash, NEW.position, NEW.pressing, NEW.listing, NEW.source_tracks_kind, NEW.source_tracks_count, NEW.document_failure)
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_failure_added AFTER INSERT ON import_candidate_failure
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = NEW.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_failure_removed AFTER DELETE ON import_candidate_failure
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash = OLD.content_hash;
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_failure_changed
AFTER UPDATE OF content_hash ON import_candidate_failure
WHEN OLD.content_hash IS NOT NEW.content_hash
BEGIN
    INSERT INTO import_list_replace (key)
    SELECT path FROM scan_candidate WHERE content_hash IN (OLD.content_hash, NEW.content_hash);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_standing_added AFTER INSERT ON import_candidate_standing
BEGIN
    INSERT INTO import_list_replace (key) VALUES (NEW.candidate_key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_standing_removed AFTER DELETE ON import_candidate_standing
BEGIN
    INSERT INTO import_list_replace (key) VALUES (OLD.candidate_key);
END;

CREATE TRIGGER IF NOT EXISTS import_list_on_standing_changed
AFTER UPDATE OF candidate_key, identification, save_failure_error, import_step
ON import_candidate_standing
WHEN (OLD.candidate_key, OLD.identification, OLD.save_failure_error, OLD.import_step IS NULL)
  IS NOT (NEW.candidate_key, NEW.identification, NEW.save_failure_error, NEW.import_step IS NULL)
BEGIN
    INSERT INTO import_list_replace (key) SELECT OLD.candidate_key UNION SELECT NEW.candidate_key;
END;
