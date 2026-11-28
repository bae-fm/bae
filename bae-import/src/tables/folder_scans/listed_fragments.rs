//! What the folder scans offer the import list's rows: each release's scan
//! row, with what a folder that failed validation shows, and the versions of
//! the files a chosen cover reads.

// Each release's scan row. `listed_invalid_tags` are the album title and
// credits its tags give a folder that failed validation, as a JSON array, and
// `listed_invalid_cover` is the image it leads with (`LeadingImage`).
coven::fragment!(
    ListedScan {
        listed_invalid_tags: Option<String>
    },
    r#"
    SELECT scan.watched_folder_path AS listed_root, scan.path AS listed_key,
        scan.name AS listed_name, scan.content_hash AS listed_content_hash,
        scan.file_edit_revision AS listed_revision, scan.grouping_key AS listed_grouping_key,
        scan.folder AS listed_folder, scan.invalid_reason AS listed_invalid_reason,
        scan.invalid_file_name AS listed_invalid_file_name,
        scan.invalid_damaged_audio AS listed_damaged_audio,
        scan.invalid_audio_bits AS listed_audio_bits,
        (SELECT json_array(tags.album_title, tags.credits)
         FROM scan_skipped_tags AS tags
         WHERE tags.watched_folder_path = scan.watched_folder_path
           AND tags.candidate_path = scan.path) AS listed_invalid_tags,
        @crate::tables::folder_scans::fragments::LeadingImage(parent: scan)
            AS listed_invalid_cover
    FROM scan_candidate AS scan
"#
);

// Each of a release's files as the scan read it.
coven::fragment!(
    ListedFile,
    r#"
    SELECT file.watched_folder_path AS file_root, file.candidate_path AS file_candidate,
        file.relative_path AS file_name, file.absolute_path AS file_path,
        file.size AS file_size, file.modified_at_ns AS file_modified
    FROM scan_candidate_file AS file
"#
);

// Which file each release's file-tag snapshot read its embedded cover from.
coven::fragment!(
    ListedSnapshot,
    r#"
    SELECT snapshot.watched_folder_path AS snapshot_root,
        snapshot.candidate_path AS snapshot_candidate,
        snapshot.embedded_cover_source_relative_path AS snapshot_cover_source
    FROM scan_candidate_tag_snapshot AS snapshot
"#
);

// The reading each release's file-tag snapshot took of each file.
coven::fragment!(
    ListedTagReading,
    r#"
    SELECT tags.watched_folder_path AS tags_root, tags.candidate_path AS tags_candidate,
        tags.relative_path AS tags_file, tags.file_size AS tags_size,
        tags.modified_at_ns AS tags_modified
    FROM scan_candidate_file_tag AS tags
"#
);
