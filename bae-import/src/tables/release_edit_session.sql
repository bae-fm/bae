-- Each release editor's session on a library release: the release's values
-- as the session opened on them, the form with each change the person made,
-- the catalog release applied in it and the album picked in it. An editor
-- writes its session's rows as it changes them, and its one live query reads
-- them. Closing the editor deletes its session, and no session outlives the
-- process: bae-import deletes every row when the library opens.

-- The session's release, what it opened on, and the form's album and
-- pressing fields as `import_candidate_edit` holds a draft's. `opened` is the
-- release's form, tracks and album as the session opened on them, as JSON:
-- a save checks the release against it, and the draft compares the form
-- with it.
CREATE TABLE IF NOT EXISTS release_edit_session (
    session         INTEGER PRIMARY KEY,
    release_id      TEXT NOT NULL REFERENCES releases (id) ON DELETE CASCADE,
    opened          TEXT NOT NULL CHECK (json_valid(opened) AND json_type(opened) = 'object'),
    album_title     TEXT NOT NULL,
    album_year      TEXT NOT NULL,
    year            TEXT NOT NULL,
    country         TEXT CHECK (country IS NULL OR (length(country) = 2 AND country = upper(country))),
    region          TEXT CHECK (region IS NULL OR region <> ''),
    media           TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(media) AND json_type(media) = 'array'),
    status          TEXT CHECK (status IS NULL OR status IN ('official', 'promotion', 'bootleg', 'pseudo_release', 'withdrawn', 'expunged', 'cancelled')),
    packaging       TEXT CHECK (packaging IS NULL OR packaging IN ('jewel_case', 'slim_jewel_case', 'digipak', 'cardboard_sleeve', 'other', 'keep_case', 'unpackaged', 'cassette_case', 'book', 'fatbox', 'snap_case', 'gatefold_cover', 'discbox_slider', 'super_jewel_box', 'digibook', 'plastic_sleeve', 'box', 'slidepack', 'snap_pack', 'metal_tin', 'longbox', 'clamshell_case', 'digifile', 'slipcase')),
    discogs_details TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(discogs_details) AND json_type(discogs_details) = 'array'),
    CHECK (country IS NULL OR region IS NULL)
) STRICT;

CREATE INDEX IF NOT EXISTS release_edit_session_by_release
    ON release_edit_session (release_id);

-- The form's label rows as typed, in order, as `import_candidate_edit_label`
-- holds a draft's.
CREATE TABLE IF NOT EXISTS release_edit_label (
    session              INTEGER NOT NULL,
    position             INTEGER NOT NULL CHECK (position >= 0),
    name                 TEXT NOT NULL,
    catalog_number       TEXT NOT NULL,
    musicbrainz_label_id TEXT CHECK (musicbrainz_label_id IS NULL OR musicbrainz_label_id <> ''),
    discogs_label_id     TEXT CHECK (discogs_label_id IS NULL OR discogs_label_id <> ''),
    PRIMARY KEY (session, position),
    FOREIGN KEY (session) REFERENCES release_edit_session (session) ON DELETE CASCADE
) STRICT;

-- The form's barcode rows as typed, in order.
CREATE TABLE IF NOT EXISTS release_edit_barcode (
    session  INTEGER NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    barcode  TEXT NOT NULL,
    PRIMARY KEY (session, position),
    FOREIGN KEY (session) REFERENCES release_edit_session (session) ON DELETE CASCADE
) STRICT;

-- The form's album artists: a library artist the person picked ('picked'),
-- or a name a catalog or the person gave ('credit'), as
-- `import_candidate_album_artist_assignment` holds a draft's.
CREATE TABLE IF NOT EXISTS release_edit_album_artist (
    session               INTEGER NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('picked', 'credit')),
    artist_id             TEXT,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    credit_key            TEXT,
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    join_phrase           TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
    PRIMARY KEY (session, position),
    FOREIGN KEY (session) REFERENCES release_edit_session (session) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    CHECK (
        (assignment_kind = 'picked' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL
            AND credited_as IS NULL)
        OR
        (assignment_kind = 'credit' AND artist_id IS NULL AND name IS NOT NULL AND name <> ''
            AND credit_key IS NULL)
    )
) STRICT;

CREATE INDEX IF NOT EXISTS release_edit_album_artist_by_artist
    ON release_edit_album_artist (artist_id);

-- Each track row of the form, by the library track it edits, in release
-- order: its title and whether it takes the album's artists.
CREATE TABLE IF NOT EXISTS release_edit_track (
    session                INTEGER NOT NULL,
    track_id               TEXT NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    position               INTEGER NOT NULL CHECK (position >= 0),
    title                  TEXT NOT NULL,
    artist_assignment_kind TEXT NOT NULL CHECK (artist_assignment_kind IN ('album_artists', 'explicit')),
    PRIMARY KEY (session, track_id),
    UNIQUE (session, position),
    FOREIGN KEY (session) REFERENCES release_edit_session (session) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS release_edit_track_by_track ON release_edit_track (track_id);

-- A form track's own artists, where it does not take the album's.
CREATE TABLE IF NOT EXISTS release_edit_track_artist (
    session               INTEGER NOT NULL,
    track_id              TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    assignment_kind       TEXT NOT NULL CHECK (assignment_kind IN ('picked', 'credit')),
    artist_id             TEXT,
    name                  TEXT,
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    credit_key            TEXT,
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    join_phrase           TEXT CHECK (join_phrase IS NULL OR join_phrase <> ''),
    PRIMARY KEY (session, track_id, position),
    FOREIGN KEY (session, track_id)
        REFERENCES release_edit_track (session, track_id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists (id) ON DELETE CASCADE,
    CHECK (
        (assignment_kind = 'picked' AND artist_id IS NOT NULL AND name IS NULL
            AND sort_name IS NULL AND musicbrainz_artist_id IS NULL AND discogs_artist_id IS NULL
            AND credited_as IS NULL)
        OR
        (assignment_kind = 'credit' AND artist_id IS NULL AND name IS NOT NULL AND name <> ''
            AND credit_key IS NULL)
    )
) STRICT;

CREATE INDEX IF NOT EXISTS release_edit_track_artist_by_artist
    ON release_edit_track_artist (artist_id);

-- What the person picked for the album the session files the release under,
-- over the one its records name, as `import_candidate_album_pick` holds a
-- candidate's. No row: the album the records name, else the album the
-- release is in. A row with an album: that library album; a row with no
-- album: a new album. Deleting a picked album deletes its pick row.
CREATE TABLE IF NOT EXISTS release_edit_album_pick (
    session  INTEGER PRIMARY KEY,
    album_id TEXT,
    FOREIGN KEY (session) REFERENCES release_edit_session (session) ON DELETE CASCADE,
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS release_edit_album_pick_by_album ON release_edit_album_pick (album_id);

-- One row once a catalog release is applied in the session. Every row below
-- holds a value the apply read, and goes with this row.
CREATE TABLE IF NOT EXISTS release_edit_applied (
    session INTEGER PRIMARY KEY,
    FOREIGN KEY (session) REFERENCES release_edit_session (session) ON DELETE CASCADE
) STRICT;

-- The applied release's mediums, in order.
CREATE TABLE IF NOT EXISTS release_edit_medium (
    session    INTEGER NOT NULL,
    position   INTEGER NOT NULL CHECK (position >= 1),
    medium     TEXT CHECK (medium IS NULL OR medium <> ''),
    side_count INTEGER NOT NULL CHECK (side_count >= 1),
    PRIMARY KEY (session, position),
    FOREIGN KEY (session) REFERENCES release_edit_applied (session) ON DELETE CASCADE
) STRICT;

-- Where the applied release places each form track: its medium, side and
-- number, and the length the catalog lists for it, unstated where it lists
-- none.
CREATE TABLE IF NOT EXISTS release_edit_track_place (
    session          INTEGER NOT NULL,
    track_id         TEXT NOT NULL,
    medium           INTEGER NOT NULL CHECK (medium >= 1),
    side             INTEGER NOT NULL CHECK (side >= 1),
    track_number     INTEGER NOT NULL CHECK (track_number >= 1),
    stated_length_ms INTEGER CHECK (stated_length_ms IS NULL OR stated_length_ms >= 0),
    PRIMARY KEY (session, track_id),
    FOREIGN KEY (session) REFERENCES release_edit_applied (session) ON DELETE CASCADE,
    FOREIGN KEY (session, track_id)
        REFERENCES release_edit_track (session, track_id) ON DELETE CASCADE,
    FOREIGN KEY (session, medium)
        REFERENCES release_edit_medium (session, position) ON DELETE CASCADE
) STRICT;

-- Who the applied release credits on a track in a role, in its order, as
-- `import_candidate_track_role` holds a draft's.
CREATE TABLE IF NOT EXISTS release_edit_track_role (
    session               INTEGER NOT NULL,
    track_id              TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL CHECK (name <> ''),
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    source                TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    role                  TEXT NOT NULL CHECK (role IN ('writer', 'composer', 'lyricist', 'librettist', 'translator', 'reviser', 'scriptwriter', 'reconstructor', 'arranger', 'producer', 'engineer', 'mixer', 'mastering', 'performer', 'instrument', 'voice', 'stated')),
    role_words            TEXT CHECK (role_words IS NULL OR role_words <> ''),
    qualifiers            TEXT NOT NULL CHECK (json_valid(qualifiers) AND json_type(qualifiers) = 'array'),
    CHECK (CASE role WHEN 'stated' THEN role_words IS NOT NULL WHEN 'instrument' THEN role_words IS NOT NULL WHEN 'voice' THEN role_words IS NOT NULL WHEN 'performer' THEN 1 ELSE role_words IS NULL END),
    PRIMARY KEY (session, track_id, position),
    FOREIGN KEY (session) REFERENCES release_edit_applied (session) ON DELETE CASCADE,
    FOREIGN KEY (session, track_id)
        REFERENCES release_edit_track (session, track_id) ON DELETE CASCADE
) STRICT;

-- Who the applied release credits in a role on the release itself, in its
-- order.
CREATE TABLE IF NOT EXISTS release_edit_release_role (
    session               INTEGER NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL CHECK (name <> ''),
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    source                TEXT NOT NULL CHECK (source IN ('musicbrainz', 'discogs')),
    role                  TEXT NOT NULL CHECK (role IN ('writer', 'composer', 'lyricist', 'librettist', 'translator', 'reviser', 'scriptwriter', 'reconstructor', 'arranger', 'producer', 'engineer', 'mixer', 'mastering', 'performer', 'instrument', 'voice', 'stated')),
    role_words            TEXT CHECK (role_words IS NULL OR role_words <> ''),
    qualifiers            TEXT NOT NULL CHECK (json_valid(qualifiers) AND json_type(qualifiers) = 'array'),
    CHECK (CASE role WHEN 'stated' THEN role_words IS NOT NULL WHEN 'instrument' THEN role_words IS NOT NULL WHEN 'voice' THEN role_words IS NOT NULL WHEN 'performer' THEN 1 ELSE role_words IS NULL END),
    PRIMARY KEY (session, position),
    FOREIGN KEY (session) REFERENCES release_edit_applied (session) ON DELETE CASCADE
) STRICT;

-- Every MusicBrainz work the applied release's tracks perform, and the works
-- those are parts of or hold, once each, in the order the catalog reached
-- them.
CREATE TABLE IF NOT EXISTS release_edit_work (
    session             INTEGER NOT NULL,
    musicbrainz_work_id TEXT NOT NULL CHECK (musicbrainz_work_id <> ''),
    position            INTEGER NOT NULL CHECK (position >= 0),
    title               TEXT NOT NULL,
    disambiguation      TEXT,
    work_type           TEXT,
    PRIMARY KEY (session, musicbrainz_work_id),
    UNIQUE (session, position),
    FOREIGN KEY (session) REFERENCES release_edit_applied (session) ON DELETE CASCADE
) STRICT;

-- An applied work's credits, in the work's order.
CREATE TABLE IF NOT EXISTS release_edit_work_credit (
    session               INTEGER NOT NULL,
    musicbrainz_work_id   TEXT NOT NULL,
    position              INTEGER NOT NULL CHECK (position >= 0),
    name                  TEXT NOT NULL CHECK (name <> ''),
    sort_name             TEXT,
    musicbrainz_artist_id TEXT,
    discogs_artist_id     TEXT,
    credited_as           TEXT CHECK (credited_as IS NULL OR credited_as <> ''),
    role                  TEXT NOT NULL CHECK (role IN ('writer', 'composer', 'lyricist', 'librettist', 'translator', 'reviser', 'scriptwriter', 'reconstructor', 'arranger', 'producer', 'engineer', 'mixer', 'mastering', 'performer', 'instrument', 'voice', 'stated')),
    role_words            TEXT CHECK (role_words IS NULL OR role_words <> ''),
    qualifiers            TEXT NOT NULL CHECK (json_valid(qualifiers) AND json_type(qualifiers) = 'array'),
    CHECK (CASE role WHEN 'stated' THEN role_words IS NOT NULL WHEN 'instrument' THEN role_words IS NOT NULL WHEN 'voice' THEN role_words IS NOT NULL WHEN 'performer' THEN 1 ELSE role_words IS NULL END),
    PRIMARY KEY (session, musicbrainz_work_id, position),
    FOREIGN KEY (session, musicbrainz_work_id)
        REFERENCES release_edit_work (session, musicbrainz_work_id) ON DELETE CASCADE
) STRICT;

-- An applied work that is part of another, in the order the catalog relates
-- them.
CREATE TABLE IF NOT EXISTS release_edit_work_part (
    session        INTEGER NOT NULL,
    position       INTEGER NOT NULL CHECK (position >= 0),
    parent_work_id TEXT NOT NULL,
    child_work_id  TEXT NOT NULL,
    PRIMARY KEY (session, position),
    UNIQUE (session, parent_work_id, child_work_id),
    FOREIGN KEY (session, parent_work_id)
        REFERENCES release_edit_work (session, musicbrainz_work_id) ON DELETE CASCADE,
    FOREIGN KEY (session, child_work_id)
        REFERENCES release_edit_work (session, musicbrainz_work_id) ON DELETE CASCADE
) STRICT;

-- The works an applied track performs, in the order it performs them.
CREATE TABLE IF NOT EXISTS release_edit_track_work (
    session             INTEGER NOT NULL,
    track_id            TEXT NOT NULL,
    position            INTEGER NOT NULL CHECK (position >= 0),
    musicbrainz_work_id TEXT NOT NULL,
    PRIMARY KEY (session, track_id, position),
    UNIQUE (session, track_id, musicbrainz_work_id),
    FOREIGN KEY (session, track_id)
        REFERENCES release_edit_track (session, track_id) ON DELETE CASCADE,
    FOREIGN KEY (session, musicbrainz_work_id)
        REFERENCES release_edit_work (session, musicbrainz_work_id) ON DELETE CASCADE
) STRICT;

-- The records the apply read, as the save writes them: the applied one first
-- (position 0), each with its catalog, its kind ('pressing' or 'album'), its
-- key, and the album a pressing record files it under.
CREATE TABLE IF NOT EXISTS release_edit_record (
    session   INTEGER NOT NULL,
    position  INTEGER NOT NULL CHECK (position >= 0),
    catalog   TEXT NOT NULL,
    kind      TEXT NOT NULL CHECK (kind IN ('pressing', 'album')),
    key       TEXT NOT NULL CHECK (key <> ''),
    album_key TEXT CHECK (album_key IS NULL OR (kind = 'pressing' AND album_key <> '')),
    PRIMARY KEY (session, position),
    FOREIGN KEY (session) REFERENCES release_edit_applied (session) ON DELETE CASCADE
) STRICT;
