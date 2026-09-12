-- Combining folders makes no numbering choice: each selected folder becomes its
-- own run of discs and track numbers restart on each, so there is no second
-- layout for the row to record.
--
-- The column is named by one of the table's CHECK constraints, so SQLite cannot
-- drop it in place and the table is rebuilt. Dropping a parent runs an implicit
-- delete that fires ON DELETE CASCADE into its children, so the members are held
-- in a temporary table across the swap and put back afterwards. That delete
-- fires no triggers, so the combined rows in scan_candidate stay where they are;
-- the trigger defined on the table is dropped with it and recreated.
CREATE TEMP TABLE candidate_combination_carry AS
    SELECT candidate_key, watched_folder_path, name, skipped, created_at, error
    FROM candidate_combination;
CREATE TEMP TABLE candidate_combination_member_carry AS
    SELECT * FROM candidate_combination_member;

DROP TABLE candidate_combination;

CREATE TABLE candidate_combination (
    candidate_key TEXT PRIMARY KEY,
    watched_folder_path TEXT NOT NULL
        REFERENCES watched_import_folders (path) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    skipped INTEGER NOT NULL DEFAULT 0 CHECK (skipped IN (0, 1)),
    created_at INTEGER NOT NULL,
    error TEXT
) STRICT;

CREATE TRIGGER remove_combination_candidate AFTER DELETE ON candidate_combination
BEGIN
    DELETE FROM scan_candidate
    WHERE source_kind = 'combination' AND path = OLD.candidate_key;
END;

INSERT INTO candidate_combination SELECT * FROM candidate_combination_carry;
INSERT INTO candidate_combination_member SELECT * FROM candidate_combination_member_carry;

DROP TABLE candidate_combination_carry;
DROP TABLE candidate_combination_member_carry;
