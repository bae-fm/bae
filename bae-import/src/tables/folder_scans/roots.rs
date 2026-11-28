//! The writes of the watched folders' scans: each root's row in
//! `folder_scan_roots`, the generation every scan takes from the sequence,
//! and the directories recorded for a root.

use super::*;

coven::write!(
    NextScanGeneration,
    r#"
    UPDATE folder_scan_generation_sequence SET last_generation = last_generation + 1
    WHERE singleton = 1
    RETURNING last_generation
"#
);

/// The next scan generation, from the counter's one row, which the schema
/// seeds. A library without the row fails the write.
pub(super) fn next_folder_scan_generation(sql: &CovenWrite<'_>) -> Result<i64, DbError> {
    Ok(sql.returning_one(&NextScanGenerationParams {})?.last_generation)
}

#[cfg(test)]
coven::write!(DeleteGenerationCounter, r#"DELETE FROM folder_scan_generation_sequence"#);

#[cfg(test)]
impl ImportTables {
    /// Plant damage for a test: the library loses the generation counter's
    /// one row, which no write deletes.
    pub(crate) async fn delete_generation_counter_for_test(&self) -> Result<(), DbError> {
        self.store
            .call(|sql| {
                sql.run(&DeleteGenerationCounterParams {})?;
                Ok(())
            })
            .await
    }
}

// A root's row as a new scan generation opens it, with no outcome yet.
coven::write!(
    BeginRootScan(watched_folder_path: Vec<u8>, generation: i64, volume: &'static str),
    r#"
    INSERT INTO folder_scan_roots (watched_folder_path, generation, status, detail, volume)
    VALUES (:watched_folder_path, :generation, NULL, NULL, :volume)
    ON CONFLICT (watched_folder_path) DO UPDATE SET
        generation = excluded.generation, status = NULL, detail = NULL,
        unwatched_directory = NULL, unwatched_cause = NULL, volume = excluded.volume
"#
);

coven::write!(
    SetRootGeneration(watched_folder_path: &'p [u8], generation: i64),
    r#"
    UPDATE folder_scan_roots SET generation = :generation
    WHERE watched_folder_path = :watched_folder_path
"#
);

coven::write!(
    CompleteRootScan(watched_folder_path: Vec<u8>, generation: i64),
    r#"
    UPDATE folder_scan_roots SET status = 'complete', detail = NULL,
        unwatched_directory = NULL, unwatched_cause = NULL
    WHERE watched_folder_path = :watched_folder_path AND generation = :generation
"#
);

coven::write!(
    LeaveRootUnwatched(
        watched_folder_path: Vec<u8>,
        generation: i64,
        detail: &'p str,
        unwatched_directory: Vec<u8>,
        unwatched_cause: &'static str,
    ),
    r#"
    UPDATE folder_scan_roots SET status = 'unwatched', detail = :detail,
        unwatched_directory = :unwatched_directory, unwatched_cause = :unwatched_cause
    WHERE watched_folder_path = :watched_folder_path AND generation = :generation
"#
);

coven::write!(
    FailRootScan(watched_folder_path: Vec<u8>, generation: i64, detail: String),
    r#"
    UPDATE folder_scan_roots SET status = 'failed', detail = :detail,
        unwatched_directory = NULL, unwatched_cause = NULL
    WHERE watched_folder_path = :watched_folder_path AND generation = :generation
"#
);

coven::write!(
    DeleteRootDirectories(watched_folder_path: &'p [u8]),
    r#"DELETE FROM folder_scan_directory WHERE watched_folder_path = :watched_folder_path"#
);

// The directories recorded for a root at or below one folder, by the byte
// range of the paths below it.
coven::write!(
    DeleteDirectoriesAtOrBelow(
        watched_folder_path: &'p [u8],
        folder: &'p [u8],
        below_first: &'p [u8],
        below_past: &'p [u8],
    ),
    r#"
    DELETE FROM folder_scan_directory
    WHERE watched_folder_path = :watched_folder_path
        AND (path = :folder OR (path >= :below_first AND path < :below_past))
"#
);

coven::write!(
    InsertScanDirectory(watched_folder_path: &'p [u8], path: Vec<u8>, modified_at: i64),
    r#"
    INSERT INTO folder_scan_directory (watched_folder_path, path, modified_at)
    VALUES (:watched_folder_path, :path, :modified_at)
"#
);
