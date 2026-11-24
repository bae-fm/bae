-- The copies of an import candidate's pictures that this device made, so a
-- screen that shows a picture again reads the copy and not the picture's file
-- on the person's disk or share. A copy is of a file the scan read, at the
-- version it read (size and modification time), at one step's size: a file
-- replaced is another picture, with no row until a screen asks for it.
-- `source_path` is coven's bytes for the file the picture is read from, the
-- image file itself or the audio file whose tags carry the art. A candidate's
-- copies go with it: with its scan rows, when a scan stops finding it, and
-- when it is imported or set aside (`picture_copies.rs`). Device-local, never
-- synced.
CREATE TABLE IF NOT EXISTS import_picture_copy (
    watched_folder_path   BLOB NOT NULL,
    candidate_path        BLOB NOT NULL,
    source_kind           TEXT NOT NULL CHECK (source_kind IN ('folder_file', 'embedded_art')),
    source_path           BLOB NOT NULL,
    source_size           INTEGER NOT NULL CHECK (source_size >= 0),
    source_modified_at_ns INTEGER NOT NULL CHECK (source_modified_at_ns >= 0),
    step                  INTEGER NOT NULL CHECK (step IN (192, 384, 768, 1536)),
    format                TEXT NOT NULL CHECK (format IN ('jpeg', 'png')),
    bytes                 BLOB NOT NULL,
    PRIMARY KEY (watched_folder_path, candidate_path, source_kind, source_path,
                 source_size, source_modified_at_ns, step),
    FOREIGN KEY (watched_folder_path, candidate_path)
        REFERENCES scan_candidate (watched_folder_path, path) ON DELETE CASCADE
) STRICT;
