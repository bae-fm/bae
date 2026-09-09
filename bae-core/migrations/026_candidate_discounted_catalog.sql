-- The catalog numbers a candidate's own text carries that the person struck
-- out. Ranking asks, of every result a run brought back, whether the folder
-- prints that result's catalog number; a number in here is one the folder
-- prints and the person has said means nothing, so the result carrying it
-- earns no catalog agreement from the text.
--
-- Its own table rather than a column on the chosen numbers, because it answers
-- a different question about a different set of values: the chosen numbers are
-- query strings, dispatched in the order they were chosen, and these judge
-- answers already in hand. A number can be both — looked up and struck out —
-- and neither list constrains the other.
--
-- A set: nothing dispatches on the order, so the value is the primary key and
-- a number cannot be struck out twice.
CREATE TABLE import_candidate_discounted_catalog (
    content_hash TEXT NOT NULL,
    value        TEXT NOT NULL,
    PRIMARY KEY (content_hash, value),
    FOREIGN KEY (content_hash) REFERENCES import_candidate_lookup_choices (content_hash) ON DELETE CASCADE
) STRICT;
