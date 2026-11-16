-- When a write changes the device's language, or the version of the
-- collation data bae writes name keys with (`name_sort`, §2.1), this trigger
-- writes every listing's key columns again from the rows they come from, in
-- the same write: it lists every album again, whose refresh triggers write
-- the album's links' and releases' keys, writes the artists' and composers'
-- keys from their names, and lists the composers' pages' rows again.
CREATE TRIGGER IF NOT EXISTS name_keys_rewritten AFTER UPDATE OF language, data_version ON name_sort
BEGIN
    INSERT INTO album_listing_refresh (album_id) SELECT album_id FROM album_listing;
    UPDATE artist_listing SET
        name_sort_key = bae_name_key(artists.name, NEW.language),
        sort_name_sort_key = bae_name_key(COALESCE(artists.sort_name, artists.name), NEW.language)
    FROM artists WHERE artists.id = artist_listing.artist_id;
    UPDATE composer_listing SET name_sort_key = bae_name_key(artists.name, NEW.language)
    FROM artists WHERE artists.id = composer_listing.artist_id;
    INSERT INTO composer_detail_work_refresh (artist_id, work_id)
    SELECT DISTINCT artist_id, item_id FROM composer_detail_row WHERE section = 0;
    INSERT INTO composer_detail_credit_refresh (credit_id)
    SELECT DISTINCT item_id FROM composer_detail_row WHERE section <> 0;
END;
