-- Provider documents are immutable within an applied draft. The shared lookup
-- cache may change independently of this candidate's source credits.
CREATE TABLE import_candidate_applied_source (
    content_hash TEXT PRIMARY KEY,
    snapshot TEXT NOT NULL CHECK (json_valid(snapshot)),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;
