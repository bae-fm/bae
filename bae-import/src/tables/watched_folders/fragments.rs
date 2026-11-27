//! What the watched folders offer other reads.

sqlive_compose::fragment!(
    WatchedFolderRows { watched_path: Vec<u8> },
    r#"
    SELECT watched.path AS watched_path, watched.position
    FROM watched_import_folders AS watched
"#
);
