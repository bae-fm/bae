//! File attachment transitions commit every file of a release together.
use super::*;

impl Database {
    pub(crate) async fn attach_release_files(&self, release_id: &str) -> Result<(), DbError> {
        let files = self.get_files_for_release(release_id).await?;
        if files.is_empty() || files.iter().any(|file| file.attached) {
            return Err(DbError::Message("release is not wholly local".into()));
        }
        let mut prepared = Vec::with_capacity(files.len());
        for file in files {
            let source = self
                .external_blob(&file.id)
                .await?
                .ok_or_else(|| DbError::FileNotUploaded(file.id.clone()))?;
            validate_original(&source).await?;
            let attachment = coven::prepare_user_file(&source.path, |_| {}).await?;
            validate_original(&source).await?;
            prepared.push((file.id, source, attachment));
        }
        let release_id = release_id.to_string();
        self.call_sql(move |sql| {
            validate_file_set(
                &sql,
                &release_id,
                prepared.iter().map(|(id, _, _)| id.as_str()),
            )?;
            for (id, source, attachment) in prepared {
                let current: (Option<String>, Option<String>, bool) = sql.query_row(
                    "SELECT l.path, l.modified_at, f.hash IS NOT NULL FROM release_files f \
                     LEFT JOIN local_release_files l ON l.file_id=f.id WHERE f.id=?",
                    [&id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?;
                let path = source
                    .path
                    .to_str()
                    .ok_or_else(|| DbError::Message("local path is not UTF-8".into()))?;
                let modified = DateTime::<Utc>::from(source.modified_at).to_rfc3339();
                if current != (Some(path.to_string()), Some(modified), false) {
                    return Err(DbError::Message(format!(
                        "local file {id} changed before attachment"
                    )));
                }
                sql.register_user_file("release_files", id.as_str(), attachment)?;
                sql.execute("DELETE FROM local_release_files WHERE file_id=?", [&id])?;
            }
            Ok(())
        })
        .await
    }

    pub(crate) async fn detach_release_files(
        &self,
        release_id: &str,
        files: Vec<(String, coven::FileRef, coven::UserFile)>,
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        self.call_sql(move |sql| {
            validate_file_set(&sql, &release_id, files.iter().map(|(id, _, _)| id.as_str()))?;
            for (id, reference, source) in files {
                sql.validate_file_ref(&reference)?;
                sql.clear_user_file("release_files", id.as_str())?;
                let path = source.path.to_str().ok_or_else(|| DbError::Message("local path is not UTF-8".into()))?;
                sql.execute("INSERT INTO local_release_files (file_id,path,modified_at) VALUES (?1,?2,?3) \
                    ON CONFLICT(file_id) DO UPDATE SET path=excluded.path,modified_at=excluded.modified_at",
                    params![id, path, DateTime::<Utc>::from(source.modified_at).to_rfc3339()])?;
            }
            Ok(())
        }).await
    }
}

async fn validate_original(source: &coven::UserFile) -> Result<(), DbError> {
    let metadata = tokio::fs::metadata(&source.path).await?;
    if metadata.len() != source.size || metadata.modified()? != source.modified_at {
        return Err(DbError::LocalFileChanged(source.path.clone()));
    }
    Ok(())
}

fn validate_file_set<'a>(
    sql: &SqlContext<'_, '_>,
    release: &str,
    expected: impl Iterator<Item = &'a str>,
) -> Result<(), DbError> {
    let current: BTreeSet<String> = sql
        .query(
            "SELECT id FROM release_files WHERE release_id=?",
            [release],
            |row| row.get(0),
        )?
        .into_iter()
        .collect();
    let expected: BTreeSet<String> = expected.map(str::to_string).collect();
    if current.is_empty() || current != expected {
        return Err(DbError::Message(format!(
            "release {release} files changed during the operation"
        )));
    }
    Ok(())
}
