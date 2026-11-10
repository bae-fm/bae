-- This device's settings for the library: one row, written by each setting's
-- setter, read by what the setting decides and by the settings screens' live
-- query. Never synced: each reflects this device, its link, its network and
-- the servers it runs. A copy of the library new to this device starts at
-- the defaults below.
CREATE TABLE IF NOT EXISTS preferences (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    -- The stored Discogs key's state; NULL while no key is stored. A state
    -- stands for a key in the keyring, so settings show it without reading
    -- the keyring.
    discogs_key TEXT CHECK (discogs_key IN ('unvalidated', 'valid', 'rejected')),
    -- Which loudness a track plays at (§7.5).
    replay_gain_mode TEXT NOT NULL CHECK (replay_gain_mode IN ('off', 'track', 'album')),
    -- The preset the last save, of a track or a release, used, which the
    -- next save starts with; a preset it names is replaced or removed only in
    -- a write that leaves it naming one.
    last_save_preset TEXT NOT NULL
        REFERENCES save_preset (id) DEFERRABLE INITIALLY DEFERRED,
    -- Whether playback pauses between sides and discs, and how many seconds
    -- such a pause waits before the next side starts on its own; NULL waits
    -- for Play. The countdown is kept while pausing is off.
    pause_between_sides          INTEGER NOT NULL CHECK (pause_between_sides IN (0, 1)),
    side_pause_countdown_seconds INTEGER CHECK (side_pause_countdown_seconds IN (5, 15, 30, 45, 60)),
    -- How many files this device uploads, and downloads, at once.
    max_concurrent_uploads   INTEGER NOT NULL CHECK (max_concurrent_uploads BETWEEN 1 AND 8),
    max_concurrent_downloads INTEGER NOT NULL CHECK (max_concurrent_downloads BETWEEN 1 AND 8),
    show_remaining_time INTEGER NOT NULL CHECK (show_remaining_time IN (0, 1)),
    library_full_width  INTEGER NOT NULL CHECK (library_full_width IN (0, 1)),
    -- Where an import goes: this device, the cloud, or the cloud with a whole
    -- copy kept here.
    import_destination TEXT NOT NULL
        CHECK (import_destination IN ('this_device', 'cloud', 'cloud_pinned')),
    -- Whether a found release is identified on its own, whether what a run
    -- picks unattended is imported, and which catalogs identification asks.
    identify_automatically INTEGER NOT NULL CHECK (identify_automatically IN (0, 1)),
    import_when_identified INTEGER NOT NULL CHECK (import_when_identified IN (0, 1)),
    ask_musicbrainz        INTEGER NOT NULL CHECK (ask_musicbrainz IN (0, 1)),
    ask_discogs            INTEGER NOT NULL CHECK (ask_discogs IN (0, 1)),
    cast_enabled          INTEGER NOT NULL CHECK (cast_enabled IN (0, 1)),
    cast_transcode_format TEXT NOT NULL CHECK (cast_transcode_format IN ('mp3', 'wav')),
    -- The MCP server; its token is in the keyring.
    mcp_enabled INTEGER NOT NULL CHECK (mcp_enabled IN (0, 1)),
    mcp_port    INTEGER NOT NULL CHECK (mcp_port BETWEEN 1 AND 65535),
    -- The Subsonic server; its password is in the keyring. The address it
    -- binds is an IP address, which the setter checks.
    subsonic_enabled      INTEGER NOT NULL CHECK (subsonic_enabled IN (0, 1)),
    subsonic_port         INTEGER NOT NULL CHECK (subsonic_port BETWEEN 1 AND 65535),
    subsonic_username     TEXT NOT NULL,
    subsonic_bind_address TEXT NOT NULL,
    CHECK (NOT subsonic_enabled OR subsonic_username <> '')
) STRICT;

-- The save presets, in the order the settings list them (§4.6). The editor
-- replaces the list whole.
CREATE TABLE IF NOT EXISTS save_preset (
    id       TEXT PRIMARY KEY CHECK (trim(id) <> ''),
    position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
    name     TEXT NOT NULL UNIQUE CHECK (trim(name) <> ''),
    -- A lossless codec writes at a bit depth, a lossy one at a bitrate.
    codec        TEXT NOT NULL CHECK (codec IN ('flac', 'mp3', 'aac', 'opus_ogg', 'wav', 'aiff')),
    bit_depth    TEXT CHECK (bit_depth IN ('same_as_source', 'bits_16')),
    bitrate_kbps INTEGER,
    embed_cover        INTEGER NOT NULL CHECK (embed_cover IN (0, 1)),
    CHECK ((codec IN ('flac', 'wav', 'aiff')) = (bit_depth IS NOT NULL)),
    CHECK ((codec IN ('flac', 'wav', 'aiff')) = (bitrate_kbps IS NULL)),
    -- The bitrates each encoder writes (SaveFormat::bitrates_kbps).
    CHECK (codec <> 'mp3' OR bitrate_kbps IN (
        32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320)),
    CHECK (codec <> 'aac' OR bitrate_kbps IN (96, 128, 160, 192, 256, 320)),
    CHECK (codec <> 'opus_ogg' OR bitrate_kbps IN (64, 96, 128, 160, 192, 256))
) STRICT;

-- The filename pattern every save names its files by, whatever the format:
-- its tokens in order.
CREATE TABLE IF NOT EXISTS save_filename_token (
    position INTEGER PRIMARY KEY CHECK (position >= 0),
    token    TEXT NOT NULL CHECK (token IN (
        'title', 'artist', 'album', 'year', 'track_number', 'disc_number', 'track_total'))
) STRICT;

-- The folder name pattern a release save names its folder by: its tokens in
-- order.
CREATE TABLE IF NOT EXISTS save_folder_token (
    position INTEGER PRIMARY KEY CHECK (position >= 0),
    token    TEXT NOT NULL CHECK (token IN ('artist', 'album', 'year', 'label', 'catalog_number'))
) STRICT;

INSERT OR IGNORE INTO save_preset (
    id, position, name, codec, bit_depth, bitrate_kbps, embed_cover)
VALUES
    ('flac', 0, 'FLAC', 'flac', 'same_as_source', NULL, 1),
    ('mp3', 1, 'MP3', 'mp3', NULL, 320, 1);
INSERT OR IGNORE INTO save_filename_token (position, token)
VALUES (0, 'track_number'), (1, 'title');
INSERT OR IGNORE INTO save_folder_token (position, token)
VALUES (0, 'artist'), (1, 'album');
INSERT OR IGNORE INTO preferences (
    singleton, discogs_key, replay_gain_mode, last_save_preset, pause_between_sides,
    side_pause_countdown_seconds,
    max_concurrent_uploads, max_concurrent_downloads, show_remaining_time,
    library_full_width, import_destination, identify_automatically,
    import_when_identified, ask_musicbrainz, ask_discogs, cast_enabled,
    cast_transcode_format, mcp_enabled, mcp_port, subsonic_enabled, subsonic_port,
    subsonic_username, subsonic_bind_address)
VALUES (
    1, NULL, 'off', 'flac', 1, NULL, 3, 3, 0, 0, 'cloud_pinned', 1, 0, 1, 1, 0, 'mp3', 0,
    47777, 0, 4533, '', '127.0.0.1');
