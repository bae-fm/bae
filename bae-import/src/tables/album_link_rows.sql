-- What reading a MusicBrainz release group found it to be on another catalog:
-- each album a statement names. A stored release of either album reads the
-- other as one of its records, even when its own documents never reach it.
-- Device-local; reading the group again replaces its rows.
CREATE TABLE IF NOT EXISTS release_group_album_link (
    release_group       TEXT NOT NULL CHECK (release_group <> ''),
    catalog             TEXT NOT NULL CHECK (catalog <> '' AND catalog <> 'musicbrainz'),
    key                 TEXT NOT NULL CHECK (key <> ''),
    PRIMARY KEY (release_group, catalog, key)
) STRICT;
CREATE INDEX IF NOT EXISTS release_group_album_link_album
    ON release_group_album_link (catalog, key);
