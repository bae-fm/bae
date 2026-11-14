-- The covers the catalogs offered a library release's cover picker the last
-- time this device fetched them, one `release_cover_fetch` row per release
-- with the fetch's answer: whether the release is linked to a catalog at
-- all. A fetch that finds the release linked adds its covers as
-- `release_cover_entry` rows, in the order offered, each the stored
-- `RemoteCover` as JSON with its catalog beside it for the picker's source
-- filter. Device-local: any device can fetch. A later fetch replaces the
-- release's rows whole; the release's deletion takes them.
CREATE TABLE IF NOT EXISTS release_cover_fetch (
    release_id TEXT NOT NULL PRIMARY KEY REFERENCES releases(id) ON DELETE CASCADE,
    linked     INTEGER NOT NULL CHECK (linked IN (0, 1))
) STRICT;

CREATE TABLE IF NOT EXISTS release_cover_entry (
    release_id TEXT NOT NULL REFERENCES release_cover_fetch(release_id) ON DELETE CASCADE,
    position   INTEGER NOT NULL CHECK (position >= 0),
    source     TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    cover      TEXT NOT NULL CHECK (json_valid(cover)),
    PRIMARY KEY (release_id, position)
) STRICT;
