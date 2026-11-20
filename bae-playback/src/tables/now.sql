-- What this device's playback is doing, as its screens show it: one row,
-- written by playback as each of these changes. Where it is in what it plays
-- is not here, since that moves with the audio, and its repeat mode is the
-- queue's. Starting playback with the library resets the row to stopped,
-- keeping the volume and mute only when it resumes what it played. Never
-- synced.
CREATE TABLE IF NOT EXISTS playback_now (
    singleton   INTEGER PRIMARY KEY CHECK (singleton = 1),
    state       TEXT NOT NULL CHECK (state IN ('stopped', 'loading', 'playing', 'paused')),
    -- The track the state names. A track the library no longer holds stays
    -- named until playback moves off it.
    track_id    TEXT,
    -- While loading: whether the track's audio is prepared yet. A playing or
    -- paused track always is.
    prepared    INTEGER CHECK (prepared IN (0, 1)),
    -- While loading: whether the person waits on storage, because one of the
    -- track's reads fetches from there.
    waits_on_storage INTEGER CHECK (waits_on_storage IN (0, 1)),
    -- The prepared track's length; NULL when it is unknown.
    duration_ms INTEGER CHECK (duration_ms >= 0),
    -- Why it is paused: by hand, or at the end of a side or disc, which the
    -- pause prompt names by its boundary's id and the heading of the side
    -- that ended (`bae_types::mediums::SideHeading`: a lettered side, with
    -- its letters, or a disc, with its number), with when the next side
    -- starts on its own (milliseconds since the epoch), NULL when it waits
    -- for Play.
    pause_reason       TEXT CHECK (pause_reason IN ('manual', 'side_ended')),
    side_boundary_id   TEXT,
    ended_side_kind    TEXT CHECK (ended_side_kind IN ('sided', 'disc')),
    ended_side         TEXT,
    side_resumes_at    INTEGER,
    volume      REAL NOT NULL CHECK (volume BETWEEN 0 AND 1),
    is_muted    INTEGER NOT NULL CHECK (is_muted IN (0, 1)),
    -- Where playback goes: this device's own output, a remote renderer, or
    -- back here after the link to the renderer was lost, with why
    -- (untranslated). The renderer is named by its id, its name, its kind,
    -- its model (NULL when it states none), its address and whether it takes
    -- a volume set, so the cast
    -- picker lists it while a new search hasn't found it.
    remote               TEXT NOT NULL CHECK (remote IN ('local', 'on', 'ended')),
    cast_receiver_id     TEXT,
    cast_receiver_name   TEXT,
    cast_receiver_kind   TEXT CHECK (cast_receiver_kind IN ('cast', 'dlna', 'airplay')),
    cast_receiver_model  TEXT,
    cast_receiver_address TEXT,
    cast_receiver_takes_volume INTEGER CHECK (cast_receiver_takes_volume IN (0, 1)),
    remote_end_detail    TEXT,
    -- The file range a preview auditions (its path as coven's bytes), and
    -- the clip's length.
    preview              TEXT NOT NULL CHECK (preview IN ('idle', 'playing', 'paused')),
    preview_path         BLOB,
    preview_start_sample INTEGER CHECK (preview_start_sample >= 0),
    preview_end_sample   INTEGER CHECK (preview_end_sample >= 0),
    preview_duration_ms  INTEGER CHECK (preview_duration_ms >= 0),
    -- Why playback last failed, until it next plays or the person dismisses
    -- the failure notice: a reason the person can act on, the other device a
    -- release's audio is on (by its name, and whether it is still in the
    -- library, removed, or replaced), the AirPlay receiver that no longer
    -- accepts this device (named as the cast_receiver_* columns name one,
    -- so Pair again… casts to it), or bae's own or the renderer's
    -- failure with its untranslated detail. Playback writes a later failure
    -- over the earlier one.
    failure         TEXT CHECK (failure IN (
        'sync_disconnected', 'on_other_device', 'original_missing',
        'audio_output', 'internal', 'renderer')),
    failure_problem TEXT CHECK (failure_problem IN (
        'device_unavailable', 'unsupported_format', 'airplay_refused',
        'airplay_pairing_lost')),
    failure_detail  TEXT,
    failure_device       TEXT,
    failure_device_state TEXT CHECK (failure_device_state IN ('active', 'removed', 'replaced')),
    failure_receiver_id   TEXT,
    failure_receiver_name TEXT,
    failure_receiver_kind TEXT CHECK (failure_receiver_kind IN ('cast', 'dlna', 'airplay')),
    failure_receiver_model TEXT,
    failure_receiver_address TEXT,
    failure_receiver_takes_volume INTEGER CHECK (failure_receiver_takes_volume IN (0, 1)),
    CHECK ((state = 'stopped') = (track_id IS NULL)),
    CHECK ((state = 'loading') = (prepared IS NOT NULL)),
    CHECK ((state = 'loading') = (waits_on_storage IS NOT NULL)),
    CHECK (duration_ms IS NULL OR state IN ('playing', 'paused') OR prepared = 1),
    CHECK ((state = 'paused') = (pause_reason IS NOT NULL)),
    CHECK ((pause_reason IS 'side_ended') = (side_boundary_id IS NOT NULL)),
    CHECK ((side_boundary_id IS NULL) = (ended_side_kind IS NULL)
        AND (side_boundary_id IS NULL) = (ended_side IS NULL)),
    CHECK (side_resumes_at IS NULL OR side_boundary_id IS NOT NULL),
    CHECK ((remote = 'local') = (cast_receiver_id IS NULL)
        AND (remote = 'local') = (cast_receiver_name IS NULL)
        AND (remote = 'local') = (cast_receiver_kind IS NULL)
        AND (remote = 'local') = (cast_receiver_address IS NULL)
        AND (remote = 'local') = (cast_receiver_takes_volume IS NULL)
        AND (remote != 'local' OR cast_receiver_model IS NULL)),
    CHECK ((remote = 'ended') = (remote_end_detail IS NOT NULL)),
    CHECK ((preview = 'idle') = (preview_path IS NULL)
        AND (preview = 'idle') = (preview_start_sample IS NULL)
        AND (preview = 'idle') = (preview_duration_ms IS NULL)),
    CHECK (preview_end_sample IS NULL OR preview_path IS NOT NULL),
    CHECK ((failure IS 'audio_output') = (failure_problem IS NOT NULL)),
    CHECK ((failure IS 'internal' OR failure IS 'renderer') = (failure_detail IS NOT NULL)),
    CHECK ((failure IS 'on_other_device') = (failure_device IS NOT NULL)
        AND (failure IS 'on_other_device') = (failure_device_state IS NOT NULL)),
    CHECK ((failure_problem IS 'airplay_pairing_lost') = (failure_receiver_id IS NOT NULL)
        AND (failure_problem IS 'airplay_pairing_lost') = (failure_receiver_name IS NOT NULL)
        AND (failure_problem IS 'airplay_pairing_lost') = (failure_receiver_kind IS NOT NULL)
        AND (failure_problem IS 'airplay_pairing_lost') = (failure_receiver_address IS NOT NULL)
        AND (failure_problem IS 'airplay_pairing_lost') = (failure_receiver_takes_volume IS NOT NULL)
        AND (failure_problem IS 'airplay_pairing_lost' OR failure_receiver_model IS NULL))
) STRICT;

INSERT OR IGNORE INTO playback_now (singleton, state, volume, is_muted, remote, preview)
VALUES (1, 'stopped', 1.0, 0, 'local', 'idle');
