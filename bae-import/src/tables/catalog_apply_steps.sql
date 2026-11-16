-- The network read each apply of a catalog release waits on now, by the id
-- the app's apply was given: an import pick's or the release editor's. The
-- apply writes the row as the read starts and deletes it as the read ends,
-- so a row exists only while its read runs. No apply outlives the process,
-- so the table is emptied as the library opens.
CREATE TABLE IF NOT EXISTS import_catalog_apply_step (
    apply_id TEXT PRIMARY KEY,
    step     TEXT NOT NULL CHECK (step IN ('reading_release', 'fetching_cover')),
    catalog  TEXT NOT NULL CHECK (catalog IN (
        'musicbrainz', 'discogs', 'allmusic', 'apple_music', 'bandcamp', 'deezer',
        'genius', 'musik_sammler', 'rateyourmusic', 'spotify', 'wikidata'))
) STRICT;
