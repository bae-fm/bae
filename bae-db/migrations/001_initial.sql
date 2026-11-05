-- bae's people, membership and sync status tables, part of the schema coven
-- applies as host migration 1.

-- Profiles retain the stable member id even after membership removal.
-- `avatar_id` is the profile picture's coven file (`coven_files`); NULL is
-- no picture.
CREATE TABLE people (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    private_circle TEXT NOT NULL,
    history_sharing TEXT NOT NULL CHECK (history_sharing IN ('private', 'household')),
    avatar_id TEXT
) STRICT;
-- Profiles list by name.
CREATE INDEX people_name ON people(name, id);
-- A profile by its picture's file, which coven's pending records name.
CREATE INDEX people_avatar ON people(avatar_id);

-- Ratings identify one person's judgment of an album or one release's track.
CREATE TABLE ratings (
    id TEXT PRIMARY KEY NOT NULL,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    album_id TEXT REFERENCES albums(id) ON DELETE CASCADE,
    track_id TEXT REFERENCES tracks(id) ON DELETE CASCADE,
    stars INTEGER NOT NULL CHECK (stars BETWEEN 1 AND 5),
    CHECK ((album_id IS NULL) <> (track_id IS NULL)),
    UNIQUE (person_id, album_id),
    UNIQUE (person_id, track_id)
) STRICT;

CREATE TABLE playlists (
    id TEXT PRIMARY KEY NOT NULL,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    audience TEXT NOT NULL
) STRICT;
-- Playlists list by name.
CREATE INDEX playlists_name ON playlists(name, id);

-- The position id distinguishes repeated tracks and concurrent insertions.
-- Integer order is not unique: ties retain both entries, ordered by position id.
CREATE TABLE playlist_entries (
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    position_id TEXT NOT NULL,
    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (playlist_id, position_id)
) STRICT;
-- A playlist's entries read in order, and its last position found for an
-- append, along the playlist's own run of this index.
CREATE INDEX playlist_entries_position ON playlist_entries(playlist_id, position, position_id);

CREATE TABLE history (
    id TEXT PRIMARY KEY NOT NULL,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    played_ms INTEGER NOT NULL CHECK (played_ms > 0),
    audience TEXT NOT NULL
) STRICT;
CREATE INDEX history_started_at ON history(started_at);

-- Before storage establishes a person, listening stays private on this device.
-- Creating the profile transfers these plays to its private circle atomically.
CREATE TABLE local_history (
    id TEXT PRIMARY KEY NOT NULL,
    track_id TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    played_ms INTEGER NOT NULL CHECK (played_ms > 0)
) STRICT;

-- The profile this device's person asked for while their private circle is
-- being made: the operation coven journaled for the circle (as
-- `coven_operations` lists it), the circle it makes, and the name they gave.
-- The write that journals the circle inserts it. Once the circle's entry
-- lands, the write that creates the profile from it, and moves
-- `local_history` into the circle, deletes it; so does the write that
-- acknowledges a circle the library refused. A person asks for one profile,
-- so there is at most one. Never synced.
CREATE TABLE profile_request (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    operation INTEGER NOT NULL,
    circle TEXT NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0)
) STRICT;

-- What each admin change this device asked for changes, which coven's
-- `coven_operations` doesn't say: keyed by the change's operation there, whose
-- kind says which of these columns it uses and whose outcome says how it
-- stands. A rename (`RenameStore`) has the new `name`; a role change
-- (`SetRole`) the `member` and the `role`; a person's removal
-- (`RemoveMember`) the `member`; a device's removal (`RemoveDevice`) the
-- `device`, its eight bytes as `coven_devices` holds them; an approval of a
-- sent invite's request (`Invite`, or `ReplaceMemberKeys` for a replacement
-- invite) the `member` it lets in and, as `name`, the name the invite gave
-- them, none for a replacement. The write that
-- journals the change inserts it; the write that acknowledges the change's
-- outcome deletes it. Never synced.
CREATE TABLE admin_operations (
    operation INTEGER PRIMARY KEY NOT NULL,
    member TEXT,
    device BLOB,
    role TEXT CHECK (role IN ('admin', 'member')),
    name TEXT
) STRICT;

-- Why this device's last attempt to start sync failed, while it stands: a
-- start that failed before coven's loop ran, such as a keychain locked for
-- the session, which coven's own status can't name. Written when starting
-- fails; deleted when a reconnect's start returns, and by the trigger below
-- as any start gets under way, a storage setup's among them. The sync status
-- shows it beside coven's. Emptied as the library opens: no attempt outlives
-- the process that made it. Never synced.
CREATE TABLE IF NOT EXISTS sync_start_failure (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    -- The failure's class, as the status line words it: one of the classes
    -- coven's sync errors map to, or `internal` for a start that panicked.
    category TEXT NOT NULL CHECK (category IN (
        'database', 'config', 'internal', 'sync_update_required',
        'device_reset_required', 'device_removed', 'membership', 'keyring',
        'keyring_locked', 'keychain_entitlement_missing',
        'saved_restore_codes_full', 'saved_restore_code_too_large', 'storage'
    )),
    -- For a storage failure, coven's reason, which the line names.
    storage_failure TEXT CHECK (storage_failure IN (
        'provider_unavailable', 'access_not_registered', 'access_not_granted',
        'dropbox_app_folder_not_shareable', 'catch_up_required', 'network',
        'authentication', 'permission_denied', 'not_found', 'already_exists',
        'conflict', 'container_not_found', 'region_mismatch', 'store_exists',
        'clock_skew', 'own_bucket_required', 'quota_exceeded', 'rate_limited',
        'invalid_configuration', 'refused', 'protocol', 'invalid_path',
        'invalid_range', 'not_store_owner', 'invitee_is_store_owner',
        'account_id_unavailable', 'invitation_mismatch', 'session_mismatch',
        'session_expired', 'session_identity_unknown', 'invalid_part',
        'single_request_too_large', 'copy_too_large', 'encoding', 'file',
        'member_keys_missing'
    )),
    -- For an S3 location that can't hold a store needing its own bucket:
    -- whether it names a base prefix or the bucket holds other objects.
    own_bucket_reason TEXT CHECK (own_bucket_reason IN ('prefix', 'occupied')),
    -- For a sign-in that lacks access: the access it must have.
    required_access TEXT CHECK (required_access IN (
        'dropbox_app_folder', 'dropbox_full', 'one_drive'
    )),
    -- For a body past what the provider takes in one request or one copy:
    -- its size and that limit, in bytes.
    size_bytes INTEGER CHECK (size_bytes >= 0),
    limit_bytes INTEGER CHECK (limit_bytes >= 0),
    -- The failure's untranslated text, with every cause below it.
    detail TEXT NOT NULL,
    CHECK ((storage_failure IS NOT NULL) = (category = 'storage')),
    CHECK ((own_bucket_reason IS NOT NULL)
        = (storage_failure IS 'own_bucket_required')),
    CHECK ((required_access IS NOT NULL) = coalesce(
        storage_failure IN ('access_not_registered', 'access_not_granted'), 0
    )),
    CHECK ((size_bytes IS NOT NULL) = coalesce(
        storage_failure IN ('single_request_too_large', 'copy_too_large'), 0
    )),
    CHECK ((limit_bytes IS NOT NULL) = (size_bytes IS NOT NULL))
) STRICT;

-- A start that got under way ends the failed start before it, in the
-- transaction that starts it: coven counts every start of sync in its sync
-- status row, a setup's among them, even one whose passes end the way the
-- last one did. A setup starts sync inside coven, so no start of bae's
-- returns to clear the row, and a status that says sync runs never shows
-- beside a start that failed before.
CREATE TRIGGER IF NOT EXISTS sync_start_failure_ended
AFTER UPDATE OF starts ON coven_sync_status
WHEN NEW.starts IS NOT OLD.starts
BEGIN
    DELETE FROM sync_start_failure;
END;
