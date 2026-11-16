-- Each row holds the network read that an apply of a catalog release is
-- waiting on, keyed by the id the library's handle gave the apply when the
-- import pick or the release editor started it. The apply writes the row as
-- the read starts and deletes it as the read ends, so a row exists only
-- while its read runs. No apply outlives the process, so bae-import deletes
-- every row when the library opens.
CREATE TABLE IF NOT EXISTS import_catalog_apply_step (
    apply_id TEXT PRIMARY KEY,
    step     TEXT NOT NULL CHECK (step IN ('reading_release', 'fetching_cover')),
    catalog  TEXT NOT NULL CHECK (catalog IN (
        'musicbrainz', 'discogs', 'allmusic', 'apple_music', 'bandcamp', 'deezer',
        'genius', 'musik_sammler', 'rateyourmusic', 'spotify', 'wikidata'))
) STRICT;
