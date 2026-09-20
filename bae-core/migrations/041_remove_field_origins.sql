-- Editable values stand on their own; applying metadata replaces them.
ALTER TABLE releases DROP COLUMN album_title_origin;
ALTER TABLE releases DROP COLUMN album_year_origin;
ALTER TABLE releases DROP COLUMN year_origin;
ALTER TABLE releases DROP COLUMN format_origin;
ALTER TABLE releases DROP COLUMN label_origin;
ALTER TABLE releases DROP COLUMN catalog_number_origin;
ALTER TABLE releases DROP COLUMN country_origin;
ALTER TABLE releases DROP COLUMN barcode_origin;
ALTER TABLE import_candidate_edit DROP COLUMN album_title_origin;
ALTER TABLE import_candidate_edit DROP COLUMN album_year_origin;
ALTER TABLE import_candidate_edit DROP COLUMN year_origin;
ALTER TABLE import_candidate_edit DROP COLUMN format_origin;
ALTER TABLE import_candidate_edit DROP COLUMN label_origin;
ALTER TABLE import_candidate_edit DROP COLUMN catalog_number_origin;
ALTER TABLE import_candidate_edit DROP COLUMN country_origin;
ALTER TABLE import_candidate_edit DROP COLUMN barcode_origin;
