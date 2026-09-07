-- Where on its image a barcode or catalog number was read: the box the
-- detector drew around it, as fractions of the image's width and height with
-- the origin at the top-left corner. A surface crops the image to it to show
-- the printed value itself. NULL where the origin is not an image, and for a
-- detector that reports payloads alone. All four present or all four absent.
ALTER TABLE import_candidate_signal_value ADD COLUMN region_x REAL;
ALTER TABLE import_candidate_signal_value ADD COLUMN region_y REAL;
ALTER TABLE import_candidate_signal_value ADD COLUMN region_width REAL;
ALTER TABLE import_candidate_signal_value ADD COLUMN region_height REAL;
