//! Each watched folder's scan as a test reads it whole: its generation,
//! status and every stored entry.

use super::read::RowSources;
use super::*;

impl Database {
    pub async fn load_folder_scan_snapshots(&self) -> Result<Vec<DbFolderScanSnapshot>, DbError> {
        self.read(move |sql| load_folder_scan_snapshots_on(&sql))
            .process(|process| process())
            .await
    }
}

fn load_folder_scan_snapshots_on(
    sql: &SqlReadContext<'_>,
) -> Result<impl FnOnce() -> Result<Vec<DbFolderScanSnapshot>, DbError> + Send + 'static, DbError> {
    let roots = sql.query(
        "SELECT roots.watched_folder_path, roots.generation, roots.status, roots.error, \
                COUNT(candidate.path) \
         FROM folder_scan_roots AS roots \
         LEFT JOIN scan_candidate AS candidate \
           ON candidate.watched_folder_path = roots.watched_folder_path \
          AND candidate.generation = roots.generation \
         GROUP BY roots.watched_folder_path, roots.generation, roots.status, roots.error \
         ORDER BY roots.watched_folder_path",
        [],
        |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    )?;

    let mut snapshots = Vec::with_capacity(roots.len());
    for (watched_folder_path, generation, status, error, found_count) in roots {
        let generation = u64::try_from(generation).map_err(|error| {
            DbError::Message(format!(
                "folder scan root {} has a negative generation: {error}",
                coven::path_text(&watched_folder_path)
            ))
        })?;
        let status = match (status.as_str(), error) {
            ("scanning", None) => crate::import::FolderScanStatus::Scanning {
                found_count: columns::to_u64(found_count, "current folder-scan candidate count")?,
            },
            ("complete", None) => crate::import::FolderScanStatus::Complete,
            ("failed", Some(error)) => crate::import::FolderScanStatus::Failed { error },
            (status, error) => {
                return Err(DbError::Message(format!(
                    "folder scan root {} has invalid status {status:?} and error {error:?}",
                    coven::path_text(&watched_folder_path)
                )))
            }
        };
        let items = load_folder_scan_items_on(sql, &watched_folder_path, RowSources::Any)?;
        snapshots.push((watched_folder_path, generation, status, items));
    }
    Ok(move || {
        snapshots
            .into_iter()
            .map(|(watched_folder_path, generation, status, items)| {
                Ok(DbFolderScanSnapshot {
                    watched_folder_path,
                    generation,
                    status,
                    items: items()?,
                })
            })
            .collect()
    })
}
