-- A provider failure is a terminal identification outcome until a person asks
-- to retry it. The typed failures are serialized as one value because no query
-- dispatches on its internals; queue placement needs only the row's presence.
CREATE TABLE import_candidate_identify_failure (
    content_hash             TEXT PRIMARY KEY,
    failures_json            TEXT NOT NULL CHECK (
        json_valid(failures_json)
        AND json_type(failures_json) = 'array'
        AND json_array_length(failures_json) > 0
    ),
    track_count              INTEGER NOT NULL CHECK (track_count >= 0),
    probed_total_duration_ms INTEGER NOT NULL CHECK (probed_total_duration_ms >= 0),
    identified_at            TEXT NOT NULL,
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;
