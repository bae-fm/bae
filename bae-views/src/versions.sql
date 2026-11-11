-- How many times the rows of each table the library's paged lists sort have
-- moved: a row inserted or deleted, or one of the values its list sorts or
-- filters it by changed. Triggers on each table advance its version in the
-- write that moves its rows. A list's read starts a window from a row an earlier read
-- found only while the versions of the tables its order reads stand where
-- they stood for that read (§9.2).
CREATE TABLE IF NOT EXISTS library_list_versions (
    source  TEXT PRIMARY KEY CHECK (source IN (
        'album_listing', 'album_artist_link', 'artist_listing', 'artist_image',
        'composer_listing', 'composer_detail_row', 'release_listing', 'library_search')),
    version INTEGER NOT NULL
) STRICT;

INSERT OR IGNORE INTO library_list_versions (source, version) VALUES
    ('album_listing', 0), ('album_artist_link', 0), ('artist_listing', 0),
    ('artist_image', 0), ('composer_listing', 0), ('composer_detail_row', 0), ('release_listing', 0),
    ('library_search', 0);

CREATE TRIGGER IF NOT EXISTS composer_listing_order_added AFTER INSERT ON composer_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_listing';
END;

CREATE TRIGGER IF NOT EXISTS composer_listing_order_removed AFTER DELETE ON composer_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_listing';
END;

CREATE TRIGGER IF NOT EXISTS composer_listing_order_moved AFTER UPDATE OF name, work_count, linked_release_count ON composer_listing
WHEN OLD.name IS NOT NEW.name
  OR OLD.work_count IS NOT NEW.work_count
  OR OLD.linked_release_count IS NOT NEW.linked_release_count
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_listing';
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_row_order_added AFTER INSERT ON composer_detail_row
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_detail_row';
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_row_order_removed AFTER DELETE ON composer_detail_row
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_detail_row';
END;

CREATE TRIGGER IF NOT EXISTS composer_detail_row_order_moved AFTER UPDATE OF group_title, group_album, group_order, title ON composer_detail_row
WHEN OLD.group_title IS NOT NEW.group_title
  OR OLD.group_album IS NOT NEW.group_album
  OR OLD.group_order IS NOT NEW.group_order
  OR OLD.title IS NOT NEW.title
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'composer_detail_row';
END;

CREATE TRIGGER IF NOT EXISTS album_listing_order_added AFTER INSERT ON album_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_listing';
END;

CREATE TRIGGER IF NOT EXISTS album_listing_order_removed AFTER DELETE ON album_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_listing';
END;

CREATE TRIGGER IF NOT EXISTS album_listing_order_moved AFTER UPDATE OF title, year, created_at, credit_sort_names, credit_artist_ids ON album_listing
WHEN OLD.title IS NOT NEW.title
  OR OLD.year IS NOT NEW.year
  OR OLD.created_at IS NOT NEW.created_at
  OR OLD.credit_sort_names IS NOT NEW.credit_sort_names
  OR OLD.credit_artist_ids IS NOT NEW.credit_artist_ids
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_listing';
END;

CREATE TRIGGER IF NOT EXISTS album_artist_link_order_added AFTER INSERT ON album_artist_link
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_artist_link';
END;

CREATE TRIGGER IF NOT EXISTS album_artist_link_order_removed AFTER DELETE ON album_artist_link
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_artist_link';
END;

CREATE TRIGGER IF NOT EXISTS album_artist_link_order_moved AFTER UPDATE OF title, year, created_at ON album_artist_link
WHEN OLD.title IS NOT NEW.title
  OR OLD.year IS NOT NEW.year
  OR OLD.created_at IS NOT NEW.created_at
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'album_artist_link';
END;

CREATE TRIGGER IF NOT EXISTS artist_listing_order_added AFTER INSERT ON artist_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_listing';
END;

CREATE TRIGGER IF NOT EXISTS artist_listing_order_removed AFTER DELETE ON artist_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_listing';
END;

CREATE TRIGGER IF NOT EXISTS artist_listing_order_moved AFTER UPDATE OF name, sort_key, album_count ON artist_listing
WHEN OLD.name IS NOT NEW.name
  OR OLD.sort_key IS NOT NEW.sort_key
  OR OLD.album_count IS NOT NEW.album_count
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_listing';
END;

-- The artists' pictures, which the album grid's artist sections carry: an
-- artist's picture version is its image row's coven file with the device that
-- holds it, so the version moves with the row and with where the file is.
CREATE TRIGGER IF NOT EXISTS artist_image_order_added AFTER INSERT ON artist_images
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_image';
END;

CREATE TRIGGER IF NOT EXISTS artist_image_order_removed AFTER DELETE ON artist_images
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_image';
END;

CREATE TRIGGER IF NOT EXISTS artist_image_order_moved AFTER UPDATE OF file ON artist_images
WHEN OLD.file IS NOT NEW.file
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_image';
END;

CREATE TRIGGER IF NOT EXISTS artist_image_file_recorded AFTER INSERT ON coven_files
WHEN EXISTS (SELECT 1 FROM artist_images WHERE file = NEW.id)
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_image';
END;

CREATE TRIGGER IF NOT EXISTS artist_image_file_placed AFTER UPDATE OF coordinates ON coven_files
WHEN EXISTS (SELECT 1 FROM artist_images WHERE file = NEW.id)
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_image';
END;

CREATE TRIGGER IF NOT EXISTS artist_image_file_removed AFTER DELETE ON coven_files
WHEN EXISTS (SELECT 1 FROM artist_images WHERE file = OLD.id)
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'artist_image';
END;

CREATE TRIGGER IF NOT EXISTS release_listing_order_added AFTER INSERT ON release_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'release_listing';
END;

CREATE TRIGGER IF NOT EXISTS release_listing_order_removed AFTER DELETE ON release_listing
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'release_listing';
END;

CREATE TRIGGER IF NOT EXISTS release_listing_order_moved AFTER UPDATE OF album_title, credit_sort_names, credit_artist_ids, media_empty, media_key, file_count, total_size, stored, queued_count, download_queued, download_failed, move_failed, pinned_count ON release_listing
WHEN OLD.album_title IS NOT NEW.album_title
  OR OLD.credit_sort_names IS NOT NEW.credit_sort_names
  OR OLD.credit_artist_ids IS NOT NEW.credit_artist_ids
  OR OLD.media_empty IS NOT NEW.media_empty
  OR OLD.media_key IS NOT NEW.media_key
  OR OLD.file_count IS NOT NEW.file_count
  OR OLD.total_size IS NOT NEW.total_size
  OR OLD.stored IS NOT NEW.stored
  OR OLD.storage_order IS NOT NEW.storage_order
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'release_listing';
END;

-- What library search finds of albums (`search.sql`) moves with an album's title,
-- an artist's name, an artist's other printed names, an album's credits and
-- the artist each credit shows as: every write changing one of them advances
-- `library_search` in `library_list_versions`, so a list narrowed to what it
-- finds, the Storage Manager's (§4.2), knows when its rows may have moved.
CREATE TRIGGER IF NOT EXISTS library_search_album_added AFTER INSERT ON albums
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_album_changed AFTER UPDATE OF title ON albums
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_added AFTER INSERT ON artists
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_changed AFTER UPDATE OF name ON artists
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_credit_added AFTER INSERT ON artist_credits
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_credit_changed AFTER UPDATE OF name ON artist_credits
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_credit_removed AFTER DELETE ON artist_credits
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_album_credit_added AFTER INSERT ON album_artists
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_album_credit_changed AFTER UPDATE OF artist_id, credit_key ON album_artists
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_album_credit_removed AFTER DELETE ON album_artists
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_shown_added AFTER INSERT ON artist_shown_as
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_shown_changed AFTER UPDATE OF shown_artist_id ON artist_shown_as
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;

CREATE TRIGGER IF NOT EXISTS library_search_artist_shown_removed AFTER DELETE ON artist_shown_as
BEGIN
    UPDATE library_list_versions SET version = version + 1 WHERE source = 'library_search';
END;
