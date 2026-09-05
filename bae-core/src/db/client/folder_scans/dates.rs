use super::*;
use crate::import::folder_scanner::FolderDate;

/// Discovery belongs to the source path, not its replaceable files or draft.
pub(super) struct CandidateDiscovery {
    first_seen_at: i64,
    folder_date: Option<FolderDate>,
}

impl CandidateDiscovery {
    pub(super) fn observe(
        sql: &SqlContext<'_, '_>,
        root: &str,
        path: &str,
        observed: Option<FolderDate>,
        now: i64,
    ) -> Result<Self, DbError> {
        let stored = sql
            .query_row(
                "SELECT first_seen_at, source_date, source_date_kind FROM scan_candidate \
             WHERE watched_folder_path = ? AND path = ?",
                params![root, path],
                |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let (first_seen_at, stored_date) = match stored {
            Some((first_seen_at, at, kind)) => {
                let date = match (at, kind.as_deref()) {
                    (None, None) => None,
                    (Some(at), Some("added_to_directory")) => {
                        Some(FolderDate::AddedToDirectory(at))
                    }
                    (Some(at), Some("created")) => Some(FolderDate::Created(at)),
                    _ => {
                        return Err(DbError::Message(format!(
                            "invalid stored folder date for {path}"
                        )))
                    }
                };
                (first_seen_at.unwrap_or(now), date)
            }
            None => (now, None),
        };
        Ok(Self {
            first_seen_at,
            // An unavailable attribute is not evidence that a previously
            // observed date ceased to exist. Actual read failures propagate
            // before this write, rather than being treated as absence.
            folder_date: observed.or(stored_date),
        })
    }

    pub(super) fn store(
        &self,
        sql: &SqlContext<'_, '_>,
        root: &str,
        path: &str,
    ) -> Result<(), DbError> {
        let date = self.folder_date.map(FolderDate::columns);
        sql.execute(
            "UPDATE scan_candidate SET first_seen_at = ?1, source_date = ?2, source_date_kind = ?3 \
             WHERE watched_folder_path = ?4 AND path = ?5 \
               AND (first_seen_at IS NOT ?1 OR source_date IS NOT ?2 OR source_date_kind IS NOT ?3)",
            params![self.first_seen_at, date.map(|(at, _)| at), date.map(|(_, kind)| kind), root, path],
        )?;
        Ok(())
    }
}
