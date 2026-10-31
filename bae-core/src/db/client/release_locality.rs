//! A release's files move to the cloud, or stop moving, every file of the
//! release together.
use super::*;

impl Database {
    /// Move every file of a wholly local release to the cloud, in one write:
    /// each original is read whole, staged with coven for the cloud and
    /// referenced by its row, and coven switches them all to the cloud
    /// together once every one is stored. `keep_here` keeps each file whole on
    /// this device once stored. Each file keeps its original here, which
    /// coven reads until then.
    pub(crate) async fn attach_release_files(
        &self,
        release_id: &str,
        keep_here: bool,
    ) -> Result<(), DbError> {
        let files = self.get_files_for_release(release_id).await?;
        if files.is_empty() {
            return Err(DbError::Message("release has no files".into()));
        }
        let mut prepared = Vec::with_capacity(files.len());
        for file in files {
            if file.file.is_some() {
                return Err(DbError::Message(format!(
                    "release {release_id} already references coven files"
                )));
            }
            let source = self
                .external_file(&file.id)
                .await?
                .ok_or_else(|| DbError::FileNotUploaded(file.id.clone()))?;
            validate_original(&source).await?;
            let original = coven::prepare_user_file(&source.path, |_| {}).await?;
            validate_original(&source).await?;
            prepared.push((file.id, source, original));
        }
        let release_id = release_id.to_string();
        let (rows, originals): (Vec<_>, Vec<_>) = prepared
            .into_iter()
            .map(|(id, source, original)| ((id, source), original))
            .unzip();
        self.write_with_files(
            move |w| {
                Ok(originals
                    .into_iter()
                    .map(|original| {
                        w.put_user_file(
                            crate::sync::RELEASE_FILES_NAMESPACE,
                            match keep_here {
                                true => coven::UserSource::CloudKeptHere(original),
                                false => coven::UserSource::Cloud(original),
                            },
                        )
                    })
                    .collect::<Vec<_>>())
            },
            move |sql, staged| {
                validate_file_set(&sql, &release_id, rows.iter().map(|(id, _)| id.as_str()))?;
                for ((id, source), file) in rows.iter().zip(staged) {
                    require_recorded_original(&sql, id, source)?;
                    let changed = sql.execute(
                        "UPDATE release_files SET file = ?1 WHERE id = ?2 AND file IS NULL",
                        params![file, id],
                    )?;
                    if changed != 1 {
                        return Err(DbError::Message(format!(
                            "release file {id} took a coven file during the move"
                        ))
                        .into());
                    }
                }
                Ok(())
            },
        )
        .await
        .map_err(Self::coven_error)
    }

    /// Stop moving each release this device is still moving to the cloud, so
    /// it plays from its originals again: one write takes every file of each
    /// such release off its coven file, and coven forgets the files and
    /// retires their uploads. Every release is checked before the write, each
    /// original on disk as recorded; a release with nothing queued here is
    /// skipped.
    pub(crate) async fn cancel_release_uploads(
        &self,
        releases: Vec<String>,
    ) -> Result<Vec<String>, DbError> {
        let mut prepared = Vec::new();
        for release in releases {
            if !self.has_pending_uploads_for_release(&release).await? {
                tracing::debug!(release, "no current audio upload to cancel");
                continue;
            }
            let files = self.moving_files(&release).await?;
            for (_, _, source) in &files {
                validate_original(source).await?;
            }
            prepared.push((release, files));
        }
        if prepared.is_empty() {
            return Ok(Vec::new());
        }
        self.call_sql(move |sql| {
            let mut detached = Vec::with_capacity(prepared.len());
            for (release, files) in prepared {
                make_local_again_on(&sql, &release, &files)?;
                detached.push(release);
            }
            Ok(detached)
        })
        .await
    }

    /// End the failed move `id` of the release `release_id`'s files, whose
    /// original changed before its upload: one write cancels it with coven
    /// and takes every file of the release off its coven file, so the release
    /// is local here again, as it was before it moved. The changed original
    /// is not checked on disk: it is why the move failed.
    pub(crate) async fn cancel_failed_release_move(
        &self,
        release_id: &str,
        id: coven::MoveId,
    ) -> Result<(), DbError> {
        let files = self.moving_files(release_id).await?;
        let release_id = release_id.to_string();
        self.write_with_files(
            |_| Ok(()),
            move |sql, ()| {
                sql.cancel_move(id)?;
                make_local_again_on(&sql, &release_id, &files)?;
                Ok(())
            },
        )
        .await
        .map_err(Self::coven_error)
    }

    /// Each file of a release moving to the cloud, with the coven file its
    /// row references and the original this device recorded for it.
    async fn moving_files(
        &self,
        release: &str,
    ) -> Result<Vec<(String, coven::FileId, coven::UserFile)>, DbError> {
        let files = self.get_files_for_release(release).await?;
        let mut moving = Vec::with_capacity(files.len());
        for file in &files {
            let coven_file = file.file.ok_or_else(|| {
                DbError::Message(format!(
                    "release {release} is not wholly moving to the cloud"
                ))
            })?;
            let source = self
                .external_file(&file.id)
                .await?
                .ok_or_else(|| DbError::FileNotUploaded(file.id.clone()))?;
            moving.push((file.id.clone(), coven_file, source));
        }
        Ok(moving)
    }
}

/// Take every file of a release moving to the cloud off its coven file,
/// once the write finds each still references the file it did and still has
/// its original recorded here. The originals' records stay, so the release
/// plays from them again.
fn make_local_again_on(
    sql: &SqlContext<'_, '_>,
    release: &str,
    files: &[(String, coven::FileId, coven::UserFile)],
) -> Result<(), DbError> {
    validate_file_set(sql, release, files.iter().map(|(id, _, _)| id.as_str()))?;
    for (id, coven_file, source) in files {
        require_recorded_original(sql, id, source)?;
        clear_coven_file_on(sql, id, coven_file)?;
    }
    Ok(())
}

/// Take the release file `id` off `coven_file`, refusing when the row
/// references another file than the one read before the write.
fn clear_coven_file_on(
    sql: &SqlContext<'_, '_>,
    id: &str,
    coven_file: &coven::FileId,
) -> Result<(), DbError> {
    let changed = sql.execute(
        "UPDATE release_files SET file = NULL WHERE id = ?1 AND file = ?2",
        params![id, coven_file],
    )?;
    if changed != 1 {
        return Err(DbError::Message(format!(
            "release file {id} changed during the operation"
        )));
    }
    Ok(())
}

/// The write still finds the original it checked on disk recorded for `id`.
fn require_recorded_original(
    sql: &SqlContext<'_, '_>,
    id: &str,
    source: &coven::UserFile,
) -> Result<(), DbError> {
    let recorded: Option<(Vec<u8>, String)> = sql
        .query_row(
            "SELECT path, modified_at FROM local_release_files WHERE file_id=?",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let modified = DateTime::<Utc>::from(source.modified_at).to_rfc3339();
    if recorded != Some((coven::path_bytes(&source.path), modified)) {
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
