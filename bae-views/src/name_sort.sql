-- The language this device orders names in, and the collation data its keys
-- were written with (§2.1). Every listed name has a key beside it, written by
-- `bae_name_key` in this language, and the lists order by the keys; a row
-- here is the one language all of them were written in. `data_version` is
-- `bae_util::name_key::DATA_VERSION` when the keys were last all written,
-- 0 before that: a device finding another one rewrites every key before it
-- serves a list. `und` is the root order, the language of a device that has
-- not been told one.
CREATE TABLE IF NOT EXISTS name_sort (
    one INTEGER PRIMARY KEY CHECK (one = 1),
    language TEXT NOT NULL,
    data_version INTEGER NOT NULL
) STRICT;

INSERT OR IGNORE INTO name_sort (one, language, data_version) VALUES (1, 'und', 0);
