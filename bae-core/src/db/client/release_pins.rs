//! Expand the file identifying a release to every file whose pin it depends on.
use super::*;

impl Database {
    pub(crate) async fn release_pin_files(
        &self,
        representatives: Vec<Option<String>>,
    ) -> Result<Vec<Option<Vec<String>>>, DbError> {
        self.read(move |sql| release_pin_files_on(&sql, &representatives))
            .await
    }

    pub(crate) fn subscribe_release_pin_files(
        &self,
        representatives: Vec<Option<String>>,
    ) -> coven::ReconfigurableLiveQuery<Vec<Option<String>>, Vec<Option<Vec<String>>>> {
        self.inner
            .handle
            .subscribe_reconfigurable(representatives, |ids, sql| {
                release_pin_files_on(&sql, ids).map_err(CovenError::from)
            })
    }
}

fn release_pin_files_on(
    sql: &SqlReadContext<'_>,
    representatives: &[Option<String>],
) -> Result<Vec<Option<Vec<String>>>, DbError> {
    let named = representatives.iter().flatten().collect::<Vec<_>>();
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for chunk in named.chunks(SQL_MAX_IN_VARS) {
        let query = format!(
            "SELECT sample.id, file.id FROM release_files sample \
            JOIN release_files file ON file.release_id=sample.release_id \
            WHERE sample.id IN ({}) ORDER BY sample.id,file.id",
            in_clause_placeholders(chunk.len())
        );
        for (sample, file) in
            sql.query(&query, rusqlite::params_from_iter(chunk.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
        {
            groups.entry(sample).or_default().push(file);
        }
    }
    Ok(representatives
        .iter()
        .map(|id| match id {
            None => Some(Vec::new()),
            Some(id) => groups.get(id).cloned(),
        })
        .collect())
}
