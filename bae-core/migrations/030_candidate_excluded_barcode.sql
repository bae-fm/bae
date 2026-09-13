-- Which barcodes a candidate's runs leave out is a list of values, not one
-- flag over the lot: a double sleeve prints the box set's code beside the
-- disc's, and only one of them names this pressing.
--
-- The header row loses `barcode_excluded` and the values move to their own
-- table, modeled on the struck-out catalog numbers. The column is named by one
-- of the table's CHECK constraints, so SQLite cannot drop it in place and the
-- table is rebuilt. Dropping a parent runs an implicit delete that fires
-- ON DELETE CASCADE into its children, so the chosen and struck-out numbers are
-- held in temporary tables across the swap and put back afterwards.
--
-- A stored "all barcodes out" names no values — the flag said nothing about
-- which codes the folder carries — so it cannot be carried over and is
-- dropped: those candidates read back asking about every code again.
CREATE TEMP TABLE import_candidate_lookup_choices_carry AS
    SELECT content_hash, disc_id_excluded FROM import_candidate_lookup_choices;
CREATE TEMP TABLE import_candidate_chosen_catalog_carry AS
    SELECT * FROM import_candidate_chosen_catalog;
CREATE TEMP TABLE import_candidate_discounted_catalog_carry AS
    SELECT * FROM import_candidate_discounted_catalog;

DROP TABLE import_candidate_lookup_choices;

CREATE TABLE import_candidate_lookup_choices (
    content_hash     TEXT PRIMARY KEY,
    disc_id_excluded INTEGER NOT NULL CHECK (disc_id_excluded IN (0, 1)),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_state (content_hash) ON DELETE CASCADE
) STRICT;

-- The barcode values the runs leave out. A set: nothing dispatches on the
-- order, so the value is part of the primary key and a code cannot be left out
-- twice.
CREATE TABLE import_candidate_excluded_barcode (
    content_hash TEXT NOT NULL,
    value        TEXT NOT NULL,
    PRIMARY KEY (content_hash, value),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_lookup_choices (content_hash) ON DELETE CASCADE
) STRICT;

INSERT INTO import_candidate_lookup_choices
    SELECT * FROM import_candidate_lookup_choices_carry;
INSERT INTO import_candidate_chosen_catalog
    SELECT * FROM import_candidate_chosen_catalog_carry;
INSERT INTO import_candidate_discounted_catalog
    SELECT * FROM import_candidate_discounted_catalog_carry;

DROP TABLE import_candidate_lookup_choices_carry;
DROP TABLE import_candidate_chosen_catalog_carry;
DROP TABLE import_candidate_discounted_catalog_carry;
