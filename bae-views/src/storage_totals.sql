-- The Storage Manager's count and total size (§4.2) under each filter, kept
-- as the releases' storage rows change, so that a read of them with no
-- search reads two rows rather than every release's.

-- For each value of `release_storage.stored` (0, local; 1, uploaded), how
-- many releases' storage rows hold it and the sum of their `total_size`.
-- The All filter's count and size are the two rows' sums. The triggers below
-- move them in the write that adds, changes or removes a storage row, which
-- is bae's own write or coven's: `release_storage`'s triggers follow coven's
-- records of the files, and a release's removal deletes its row with it.
CREATE TABLE IF NOT EXISTS storage_totals (
    stored INTEGER PRIMARY KEY CHECK (stored IN (0, 1)),
    release_count INTEGER NOT NULL CHECK (release_count >= 0),
    total_size INTEGER NOT NULL CHECK (total_size >= 0)
) STRICT;

INSERT OR IGNORE INTO storage_totals (stored, release_count, total_size) VALUES (0, 0, 0), (1, 0, 0);

CREATE TRIGGER IF NOT EXISTS storage_totals_release_added AFTER INSERT ON release_storage
BEGIN
    UPDATE storage_totals SET release_count = release_count + 1,
        total_size = total_size + NEW.total_size
    WHERE stored = NEW.stored;
END;

-- A storage row's refresh sets every column, so the trigger moves the totals
-- only when `stored` or `total_size` changed: a file queued, sent or pinned
-- writes no total.
CREATE TRIGGER IF NOT EXISTS storage_totals_release_changed AFTER UPDATE OF stored, total_size ON release_storage
WHEN OLD.stored IS NOT NEW.stored OR OLD.total_size IS NOT NEW.total_size
BEGIN
    UPDATE storage_totals SET
        release_count = release_count + (stored = NEW.stored) - (stored = OLD.stored),
        total_size = total_size
            + CASE WHEN stored = NEW.stored THEN NEW.total_size ELSE 0 END
            - CASE WHEN stored = OLD.stored THEN OLD.total_size ELSE 0 END
    WHERE stored IN (OLD.stored, NEW.stored);
END;

CREATE TRIGGER IF NOT EXISTS storage_totals_release_removed AFTER DELETE ON release_storage
BEGIN
    UPDATE storage_totals SET release_count = release_count - 1,
        total_size = total_size - OLD.total_size
    WHERE stored = OLD.stored;
END;
