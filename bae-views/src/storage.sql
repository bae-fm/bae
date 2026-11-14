-- The Storage Manager (§4.2) lists every release with its album's title and
-- credits, its media, its files' count and size, whether they are stored and
-- whether this device keeps them all pinned, how many of its files wait in
-- this device's upload queue, and when it was added, sorted by them and
-- filtered by whether they are stored. `release_listing` holds those values, kept by the triggers
-- below, with an index for each sort, alone and after `stored` for the Remote
-- and Local filters.

-- Each release, with what the Storage Manager sorts and filters it by:
-- `album_title_sort_key` and the credit key copied from its album's `album_listing`
-- row; `media_empty` and `media_key` as `release_media_sort` works them out
-- from its `release_media`; `created_at`, when the release was added; its
-- files' count and total size, `stored`, `pinned_count` and
-- `queued_count`, copied from its `release_storage` row; `download_queued`, 1 while this
-- device's download queue holds it waiting or downloading; `download_failed`,
-- 1 while that queue holds it failed; `move_failed`, copied from its
-- `release_storage` row; and `storage_order`, where it sorts by
-- storage, worked out from those in the same write: a failed download or move
-- first (0), uploading (1), pinning (2) while the download queue brings its files
-- to this device, local (3), pinned (4), in the cloud only (5).
CREATE TABLE IF NOT EXISTS release_listing (
    release_id TEXT PRIMARY KEY REFERENCES releases (id) ON DELETE CASCADE,
    album_id TEXT NOT NULL,
    album_title_sort_key BLOB NOT NULL,
    credit_sort_key BLOB,
    credit_artist_ids TEXT,
    media_empty INTEGER NOT NULL,
    media_key TEXT,
    created_at TEXT NOT NULL,
    file_count INTEGER NOT NULL,
    total_size INTEGER NOT NULL,
    stored INTEGER NOT NULL,
    pinned_count INTEGER NOT NULL,
    queued_count INTEGER NOT NULL,
    download_queued INTEGER NOT NULL CHECK (download_queued IN (0, 1)),
    download_failed INTEGER NOT NULL CHECK (download_failed IN (0, 1)),
    move_failed INTEGER NOT NULL CHECK (move_failed IN (0, 1)),
    storage_order INTEGER GENERATED ALWAYS AS (CASE
        WHEN download_failed = 1 OR move_failed = 1 THEN 0
        WHEN queued_count > 0 THEN 1
        WHEN download_queued = 1 THEN 2
        WHEN stored = 0 THEN 3
        WHEN file_count > 0 AND pinned_count = file_count THEN 4
        ELSE 5 END) VIRTUAL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_release_listing_album ON release_listing (album_id);

CREATE INDEX IF NOT EXISTS idx_release_listing_title ON release_listing (album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_title_desc ON release_listing (album_title_sort_key DESC, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_credits ON release_listing (credit_sort_key, credit_artist_ids, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_credits_desc ON release_listing (credit_sort_key DESC, credit_artist_ids DESC, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_media ON release_listing (media_empty, media_key, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_media_desc ON release_listing (media_empty, media_key DESC, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_files ON release_listing (file_count, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_files_desc ON release_listing (file_count DESC, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_size ON release_listing (total_size, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_size_desc ON release_listing (total_size DESC, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_storage ON release_listing (storage_order, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_storage_desc ON release_listing (storage_order DESC, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_added ON release_listing (created_at, album_title_sort_key, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_added_desc ON release_listing (created_at DESC, album_title_sort_key, release_id);

CREATE INDEX IF NOT EXISTS idx_release_listing_stored_title ON release_listing (stored, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_title_desc ON release_listing (stored, album_title_sort_key DESC, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_credits ON release_listing (stored, credit_sort_key, credit_artist_ids, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_credits_desc ON release_listing (stored, credit_sort_key DESC, credit_artist_ids DESC, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_media ON release_listing (stored, media_empty, media_key, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_media_desc ON release_listing (stored, media_empty, media_key DESC, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_files ON release_listing (stored, file_count, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_files_desc ON release_listing (stored, file_count DESC, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_size ON release_listing (stored, total_size, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_size_desc ON release_listing (stored, total_size DESC, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_storage ON release_listing (stored, storage_order, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_storage_desc ON release_listing (stored, storage_order DESC, album_title_sort_key, created_at, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_added ON release_listing (stored, created_at, album_title_sort_key, release_id);
CREATE INDEX IF NOT EXISTS idx_release_listing_stored_added_desc ON release_listing (stored, created_at DESC, album_title_sort_key, release_id);

-- Each release's media as the Storage Manager sorts by them: `media_empty`, 1
-- for a release stating no medium, and `media_key`, every stated medium with
-- its count, in order.
CREATE VIEW IF NOT EXISTS release_media_sort AS
SELECT r.id AS release_id,
       NOT EXISTS (SELECT 1 FROM release_media m WHERE m.release_id = r.id) AS media_empty,
       (SELECT group_concat(printf('%s:%05d', m.medium, m.count), ',' ORDER BY m.position)
        FROM release_media m WHERE m.release_id = r.id) AS media_key
FROM releases r;

-- Inserting a release id into `release_listing_refresh` rewrites that
-- release's row of `release_listing` from the release, its storage row and
-- its album's `album_listing` row; a release whose album is not listed yet
-- gets its row when the album's is written, one whose storage row is not
-- written yet when that is.
CREATE VIEW IF NOT EXISTS release_listing_refresh AS SELECT NULL AS release_id WHERE 0;

CREATE TRIGGER IF NOT EXISTS release_listing_refresh INSTEAD OF INSERT ON release_listing_refresh
BEGIN
    INSERT INTO release_listing (
        release_id, album_id, album_title_sort_key, credit_sort_key, credit_artist_ids,
        media_empty, media_key, created_at, file_count, total_size, stored, pinned_count,
        queued_count, download_queued, download_failed, move_failed)
    SELECT r.id, r.album_id, album.title_sort_key, album.credit_sort_key, album.credit_artist_ids,
           media.media_empty,
           media.media_key,
           r.created_at,
           storage.file_count,
           storage.total_size,
           storage.stored,
           storage.pinned_count,
           storage.queued_count,
           EXISTS (SELECT 1 FROM download_queue download
                   WHERE download.release_id = r.id AND download.state IN ('queued', 'active')),
           EXISTS (SELECT 1 FROM download_queue download
                   WHERE download.release_id = r.id AND download.state = 'failed'),
           storage.move_failed
    FROM releases r
    JOIN release_storage storage ON storage.release_id = r.id
    JOIN album_listing album ON album.album_id = r.album_id
    JOIN release_media_sort media ON media.release_id = r.id
    WHERE r.id = NEW.release_id
    ON CONFLICT (release_id) DO UPDATE SET
        album_id = excluded.album_id,
        album_title_sort_key = excluded.album_title_sort_key,
        credit_sort_key = excluded.credit_sort_key,
        credit_artist_ids = excluded.credit_artist_ids,
        media_empty = excluded.media_empty,
        media_key = excluded.media_key,
        file_count = excluded.file_count,
        total_size = excluded.total_size,
        stored = excluded.stored,
        pinned_count = excluded.pinned_count,
        queued_count = excluded.queued_count,
        download_queued = excluded.download_queued,
        download_failed = excluded.download_failed,
        move_failed = excluded.move_failed;
END;

-- A release arriving lists its album first. An album listed just now lists
-- every release it holds, this one with them (`release_listing_album_listed`);
-- a release joining an album already listed is refreshed by itself. Either
-- refresh waits for the release's storage row, which
-- `release_storage_release_added` writes as the release arrives, before or
-- after this trigger runs; when it runs after, the row's arrival refreshes
-- the release (`release_listing_storage_added`). So its row is refreshed once
-- whichever runs first. The two steps here share one trigger because their
-- order decides that, and SQLite names no order for several triggers on one
-- event.
CREATE TRIGGER IF NOT EXISTS release_added AFTER INSERT ON releases
BEGIN
    INSERT INTO album_listing_refresh (album_id) VALUES (NEW.album_id);
    INSERT INTO release_listing_refresh (release_id)
    SELECT NEW.id WHERE NOT EXISTS (SELECT 1 FROM release_listing WHERE release_id = NEW.id)
        AND EXISTS (SELECT 1 FROM release_storage WHERE release_id = NEW.id);
END;

-- A release moving to another album lists the album it left and the one it
-- joined first. An album listed just now lists every release it holds, this
-- one with them, so its row then names that album; otherwise the release is
-- refreshed by itself. Its row is refreshed once either way, in one trigger
-- for the same reason as `release_added`. A release's `created_at` is written
-- once, with the row.
CREATE TRIGGER IF NOT EXISTS release_changed AFTER UPDATE OF album_id ON releases
BEGIN
    INSERT INTO album_listing_refresh (album_id)
    SELECT OLD.album_id WHERE OLD.album_id IS NOT NEW.album_id
    UNION ALL
    SELECT NEW.album_id WHERE OLD.album_id IS NOT NEW.album_id;
    INSERT INTO release_listing_refresh (release_id)
    SELECT NEW.id WHERE OLD.album_id IS NEW.album_id
        OR NOT EXISTS (SELECT 1 FROM release_listing
                       WHERE release_id = NEW.id AND album_id = NEW.album_id);
END;

-- A release's media changing rewrites only its listing row's media: a
-- release arriving with its media is refreshed once by `release_added`, and
-- its media rows, written after it, set the two columns that row's refresh
-- found none for. A release no row lists yet moves nothing.
CREATE TRIGGER IF NOT EXISTS release_listing_media_added AFTER INSERT ON release_media
BEGIN
    UPDATE release_listing SET (media_empty, media_key) =
        (SELECT media_empty, media_key FROM release_media_sort
         WHERE release_id = NEW.release_id)
    WHERE release_id = NEW.release_id;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_media_changed AFTER UPDATE OF medium, count ON release_media
BEGIN
    UPDATE release_listing SET (media_empty, media_key) =
        (SELECT media_empty, media_key FROM release_media_sort media
         WHERE media.release_id = release_listing.release_id)
    WHERE release_id = NEW.release_id;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_media_removed AFTER DELETE ON release_media
BEGIN
    UPDATE release_listing SET (media_empty, media_key) =
        (SELECT media_empty, media_key FROM release_media_sort
         WHERE release_id = OLD.release_id)
    WHERE release_id = OLD.release_id;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_album_listed AFTER INSERT ON album_listing
BEGIN
    INSERT INTO release_listing_refresh (release_id)
    SELECT r.id FROM releases r
    WHERE r.album_id = NEW.album_id
      AND EXISTS (SELECT 1 FROM release_storage storage WHERE storage.release_id = r.id);
END;

CREATE TRIGGER IF NOT EXISTS release_listing_album_changed AFTER UPDATE OF title_sort_key, credit_sort_key, credit_artist_ids ON album_listing
WHEN OLD.title_sort_key IS NOT NEW.title_sort_key
  OR OLD.credit_sort_key IS NOT NEW.credit_sort_key
  OR OLD.credit_artist_ids IS NOT NEW.credit_artist_ids
BEGIN
    INSERT INTO release_listing_refresh (release_id)
    SELECT id FROM releases WHERE album_id = NEW.album_id;
END;

-- A release's storage row arriving lists the release once its album is
-- listed, and its counts changing copy into the release's listing row,
-- which sorts and filters by them. With `release_added` and
-- `release_listing_album_listed`, each refreshing a release only once the
-- other of its storage row and its album's listing row is there, a release
-- arriving is refreshed once, whichever of the triggers on `releases` runs
-- first.
CREATE TRIGGER IF NOT EXISTS release_listing_storage_added AFTER INSERT ON release_storage
BEGIN
    INSERT INTO release_listing_refresh (release_id)
    SELECT NEW.release_id WHERE EXISTS (
        SELECT 1 FROM releases r JOIN album_listing album ON album.album_id = r.album_id
        WHERE r.id = NEW.release_id);
END;

CREATE TRIGGER IF NOT EXISTS release_listing_storage_changed AFTER UPDATE OF file_count, total_size, stored, pinned_count, queued_count, move_failed ON release_storage
BEGIN
    UPDATE release_listing SET file_count = NEW.file_count, total_size = NEW.total_size,
        stored = NEW.stored, pinned_count = NEW.pinned_count,
        queued_count = NEW.queued_count,
        move_failed = NEW.move_failed
    WHERE release_id = NEW.release_id;
END;

-- A release joining, moving through or leaving this device's download queue
-- moves its listing's `download_queued` and `download_failed` flags, in the
-- transaction that changes the queue: waiting or downloading sets the first,
-- a failed download the second, and leaving the queue clears both. An entry names its release by id alone, so one whose
-- release is gone, or not listed yet, moves nothing: the listing's refresh
-- reads the queue as it writes the release's row.
CREATE TRIGGER IF NOT EXISTS release_listing_pin_queued AFTER INSERT ON download_queue
BEGIN
    UPDATE release_listing SET download_queued = NEW.state IN ('queued', 'active'),
        download_failed = NEW.state = 'failed'
    WHERE release_id = NEW.release_id;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_pin_changed AFTER UPDATE OF state ON download_queue
BEGIN
    UPDATE release_listing SET download_queued = NEW.state IN ('queued', 'active'),
        download_failed = NEW.state = 'failed'
    WHERE release_id = NEW.release_id;
END;

CREATE TRIGGER IF NOT EXISTS release_listing_pin_dequeued AFTER DELETE ON download_queue
BEGIN
    UPDATE release_listing SET download_queued = 0, download_failed = 0
    WHERE release_id = OLD.release_id;
END;
