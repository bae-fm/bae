-- Relationship facts belong to the accepted document; optional targets need
-- not have been fetched. Deleting an owner removes its references.
ALTER TABLE source_release_payloads ADD COLUMN source_group_id TEXT;
ALTER TABLE source_release_payloads ADD COLUMN document_release_id TEXT;

CREATE TABLE source_document_reference (
    source TEXT NOT NULL,
    source_release_id TEXT NOT NULL,
    target_source TEXT NOT NULL,
    target_id TEXT NOT NULL,
    PRIMARY KEY (source, source_release_id, target_source, target_id),
    FOREIGN KEY (source, source_release_id)
        REFERENCES source_release_payloads (source, source_release_id) ON DELETE CASCADE
) STRICT;
