-- The sync status's table. Part of the schema coven applies as host migration 1.

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
        'database', 'database_busy', 'database_damaged', 'config', 'internal',
        'sync_update_required',
        'device_reset_required', 'device_removed', 'membership', 'keyring',
        'keyring_locked', 'keychain_entitlement_missing',
        'saved_restore_codes_full', 'saved_restore_code_too_large', 'storage'
    )),
    -- For a storage failure, coven's reason, which the line names.
    storage_failure TEXT CHECK (storage_failure IN (
        'provider_unavailable', 'access_not_registered', 'access_not_granted',
        'dropbox_app_folder_not_shareable', 'catch_up_required', 'network',
        'corrupted', 'authentication', 'permission_denied', 'not_found',
        'already_exists', 'conflict', 'container_not_found', 'region_mismatch',
        'store_exists', 'clock_skew', 'own_location_required',
        'quota_exceeded', 'rate_limited', 'invalid_configuration', 'refused',
        'protocol', 'invalid_path', 'invalid_range', 'not_store_owner',
        'invitee_is_store_owner', 'account_id_unavailable',
        'invitation_mismatch', 'session_mismatch', 'session_expired',
        'invalid_part', 'single_request_too_large',
        'copy_too_large', 'encoding', 'file', 'member_keys_missing'
    )),
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
