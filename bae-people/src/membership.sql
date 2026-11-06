-- The admin changes this device asked for. Part of the schema coven applies as
-- host migration 1.

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
