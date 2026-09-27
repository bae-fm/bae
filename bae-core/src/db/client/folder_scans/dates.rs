use super::*;
use crate::import::folder_scanner::FolderDate;

/// What is known about when a folder was found, kept by its path on disk
/// rather than with the scan row of whichever watched folder covers it.
pub(super) struct FolderDiscovery {
    folder: String,
    first_seen_at: i64,
    folder_date: Option<FolderDate>,
    /// Whether a scan had read the folder as a release or as broken before.
    settled: bool,
}

impl FolderDiscovery {
    pub(super) fn observe(
        sql: &SqlContext<'_, '_>,
        folder: &Path,
        observed: Option<FolderDate>,
        now: i64,
    ) -> Result<Self, DbError> {
        let folder = folder.to_string_lossy().into_owned();
        let stored = sql
            .query_row(
                "SELECT first_seen_at, settled, source_date, source_date_kind \
                 FROM folder_discovery WHERE folder = ?",
                [&folder],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, bool>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?;
        let (first_seen_at, settled, stored_date) = match stored {
            Some((first_seen_at, settled, at, kind)) => {
                let date = match (at, kind.as_deref()) {
                    (None, None) => None,
                    (Some(at), Some("added_to_directory")) => {
                        Some(FolderDate::AddedToDirectory(at))
                    }
                    (Some(at), Some("created")) => Some(FolderDate::Created(at)),
                    _ => {
                        return Err(DbError::Message(format!(
                            "invalid stored folder date for {folder}"
                        )))
                    }
                };
                (first_seen_at, settled, date)
            }
            None => (now, false, None),
        };
        Ok(Self {
            folder,
            first_seen_at,
            // A date the filesystem could not give now does not erase one it
            // gave before.
            folder_date: observed.or(stored_date),
            settled,
        })
    }

    /// Whether a scan had read the folder as a release or as broken before
    /// this write.
    pub(super) fn settled(&self) -> bool {
        self.settled
    }

    /// Store what is known, marking the folder read as a release or as broken
    /// when `settles`. Writes nothing when nothing changed.
    pub(super) fn store(&self, sql: &SqlContext<'_, '_>, settles: bool) -> Result<(), DbError> {
        let date = self.folder_date.map(FolderDate::columns);
        sql.execute(
            "INSERT INTO folder_discovery \
                 (folder, first_seen_at, settled, source_date, source_date_kind) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT (folder) DO UPDATE SET \
                 first_seen_at = excluded.first_seen_at, \
                 settled = MAX(settled, excluded.settled), \
                 source_date = excluded.source_date, \
                 source_date_kind = excluded.source_date_kind \
             WHERE first_seen_at IS NOT excluded.first_seen_at \
                OR settled < excluded.settled \
                OR source_date IS NOT excluded.source_date \
                OR source_date_kind IS NOT excluded.source_date_kind",
            params![
                self.folder,
                self.first_seen_at,
                self.settled || settles,
                date.map(|(at, _)| at),
                date.map(|(_, kind)| kind)
            ],
        )?;
        Ok(())
    }
}
