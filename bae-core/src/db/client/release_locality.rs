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
                .external_file(&file.id)
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
        self.call_sql(move |sql| detach_release_files_on(&sql, &release_id, files))
            .await
    }

    /// Restore originals for releases with a currently attached file still queued.
    /// Every release is checked before the single write detaches any of them.
    pub(crate) async fn cancel_release_uploads(
        &self,
        releases: Vec<String>,
    ) -> Result<Vec<String>, DbError> {
        let uploads = self.uploads().await?;
        let mut prepared = Vec::new();
        for release in releases {
            let files = self.get_files_for_release(&release).await?;
            let mut references = Vec::new();
            for file in &files {
                if file.attached {
                    references.push((
                        file.id.clone(),
                        self.file_ref("release_files", &file.id).await?,
                    ));
                }
            }
            if !references
                .iter()
                .any(|(_, reference)| uploads.files.iter().any(|upload| upload.file == *reference))
            {
                tracing::debug!(release, "no current audio upload to cancel");
                continue;
            }
            if references.len() != files.len() {
                return Err(DbError::Message(format!(
                    "release {release} is not wholly attached"
                )));
            }
            let mut originals = Vec::with_capacity(files.len());
            for (id, reference) in references {
                let source = self
                    .external_file(&id)
                    .await?
                    .ok_or_else(|| DbError::FileNotUploaded(id.clone()))?;
                validate_original(&source).await?;
                originals.push((id, reference, source));
            }
            prepared.push((release, originals));
        }
        if prepared.is_empty() {
            return Ok(Vec::new());
        }
        self.call_sql(move |sql| {
            let mut detached = Vec::with_capacity(prepared.len());
            for (release, files) in prepared {
                detach_release_files_on(&sql, &release, files)?;
                detached.push(release);
            }
            Ok(detached)
        })
        .await
    }
}

fn detach_release_files_on(
    sql: &SqlContext<'_, '_>,
    release: &str,
    files: Vec<(String, coven::FileRef, coven::UserFile)>,
) -> Result<(), DbError> {
    validate_file_set(sql, release, files.iter().map(|(id, _, _)| id.as_str()))?;
    for (id, reference, source) in files {
        sql.validate_file_ref(&reference)?;
        sql.clear_user_file("release_files", id.as_str())?;
        let path = source
            .path
            .to_str()
            .ok_or_else(|| DbError::Message("local path is not UTF-8".into()))?;
        sql.execute(
            "INSERT INTO local_release_files (file_id,path,modified_at) VALUES (?1,?2,?3) \
             ON CONFLICT(file_id) DO UPDATE SET path=excluded.path,modified_at=excluded.modified_at",
            params![id, path, DateTime::<Utc>::from(source.modified_at).to_rfc3339()],
        )?;
    }
    Ok(())
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
