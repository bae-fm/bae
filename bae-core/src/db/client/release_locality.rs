//! File attachment transitions commit every file of a release together.
use super::*;

impl Database {
    /// Attach every file of a wholly local release for upload, in one write.
    /// Each file keeps its original here: until its upload completes, its row
    /// names no file and this device plays it from the original.
    pub(crate) async fn attach_release_files(&self, release_id: &str) -> Result<(), DbError> {
        let files = self.get_files_for_release(release_id).await?;
        if files.is_empty() {
            return Err(DbError::Message("release has no files".into()));
        }
        let mut prepared = Vec::with_capacity(files.len());
        for file in files {
            if self
                .current_file("release_files", &file.id)
                .await?
                .is_some()
            {
                return Err(DbError::Message(format!(
                    "release {release_id} is already attached"
                )));
            }
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
                require_recorded_original(&sql, &id, &source)?;
                sql.register_user_file("release_files", id.as_str(), attachment)?;
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

    /// Take back the attachment of every file of each release this device is
    /// still uploading, so it plays from its originals again. Every release is
    /// checked before the single write clears any of them; a release with
    /// nothing queued here is skipped. Clearing a file still queued abandons
    /// its attachment.
    pub(crate) async fn cancel_release_uploads(
        &self,
        releases: Vec<String>,
    ) -> Result<Vec<String>, DbError> {
        let uploads = self.uploads().await?;
        let mut prepared = Vec::new();
        for release in releases {
            let files = self.get_files_for_release(&release).await?;
            let queued = uploads.files.iter().any(|upload| {
                upload.file.table() == "release_files"
                    && files
                        .iter()
                        .any(|file| upload.file.key() == &coven::RowKey::from(file.id.as_str()))
            });
            if !queued {
                tracing::debug!(release, "no current audio upload to cancel");
                continue;
            }
            let mut attached = Vec::with_capacity(files.len());
            for file in &files {
                let reference = self
                    .current_file("release_files", &file.id)
                    .await?
                    .ok_or_else(|| {
                        DbError::Message(format!("release {release} is not wholly attached"))
                    })?;
                let source = self
                    .external_file(&file.id)
                    .await?
                    .ok_or_else(|| DbError::FileNotUploaded(file.id.clone()))?;
                validate_original(&source).await?;
                attached.push((file.id.clone(), reference, source));
            }
            prepared.push((release, attached));
        }
        if prepared.is_empty() {
            return Ok(Vec::new());
        }
        self.call_sql(move |sql| {
            let mut detached = Vec::with_capacity(prepared.len());
            for (release, files) in prepared {
                validate_file_set(&sql, &release, files.iter().map(|(id, _, _)| id.as_str()))?;
                for (id, reference, source) in files {
                    require_recorded_original(&sql, &id, &source)?;
                    sql.validate_file_ref(&reference)?;
                    sql.clear_user_file("release_files", id.as_str())?;
                }
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

/// The write still finds the original it checked on disk recorded for `id`.
fn require_recorded_original(
    sql: &SqlContext<'_, '_>,
    id: &str,
    source: &coven::UserFile,
) -> Result<(), DbError> {
    let recorded: Option<(String, String)> = sql
        .query_row(
            "SELECT path, modified_at FROM local_release_files WHERE file_id=?",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let path = source
        .path
        .to_str()
        .ok_or_else(|| DbError::Message("local path is not UTF-8".into()))?;
    let modified = DateTime::<Utc>::from(source.modified_at).to_rfc3339();
    if recorded != Some((path.to_string(), modified)) {
        return Err(DbError::Message(format!(
            "the original of {id} changed during the operation"
        )));
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
