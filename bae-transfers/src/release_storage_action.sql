-- A storage transition this process runs for a release while its files rest
-- where they are: a move to the cloud being admitted, until coven's upload
-- queue holds its files; an unpin; or making the release local. Written in
-- the write that admits it, which a release already in one refuses, and
-- deleted as it ends: in the write that finishes its work (an admitted move's
-- that queues its files, an unpin's that releases its pins, a make-local's
-- that switches its files), or in a write of its own after a failure.
-- A pin in flight is the download queue's active entry, not a row here. No
-- transition outlives the process that ran it, so the table is emptied as
-- the library opens.
CREATE TABLE IF NOT EXISTS release_storage_action (
    release_id TEXT PRIMARY KEY REFERENCES releases (id) ON DELETE CASCADE,
    action     TEXT NOT NULL CHECK (action IN ('upload', 'unpin', 'detach_audio'))
) STRICT;
