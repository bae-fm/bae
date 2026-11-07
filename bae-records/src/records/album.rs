use super::*;

/// Why making a release its album's primary was refused.
#[derive(Debug, thiserror::Error)]
pub enum PrimaryReleaseError {
    /// The release is not one of the album's: it moved to another album,
    /// or left the library, after the person picked it. Nothing was written.
    #[error("release {release_id} is not one of album {album_id}'s releases")]
    NotAlbumRelease { album_id: String, release_id: String },
    #[error(transparent)]
    Database(#[from] DbError),
}

/// A bug the store's write carried back, which the store reported as it
/// returned; a refusal holds none.
impl bae_util::task_failure::ReportedBug for PrimaryReleaseError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Database(error) => error.reported_bug(),
            Self::NotAlbumRelease { .. } => None,
        }
    }
}

impl Records {
    /// Track ids for an album's primary release. `None` means no album carries
    /// this id; `Some(vec![])` means the album exists but holds no release, or
    /// its primary release has no tracks.
    pub async fn get_primary_release_track_ids_for_album(
        &self,
        album_id: &str,
    ) -> Result<Option<Vec<String>>, DbError> {
        let album_id = album_id.to_string();
        let primary: Option<Option<String>> = self
            .store
            .read(move |sql| {
                sql.query_row(
                    &format!("SELECT {} FROM albums a WHERE a.id = ?", album_primary_release_sql()),
                    params![album_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(DbError::from)
            })
            .await?;
        match primary {
            None => Ok(None),
            Some(None) => Ok(Some(Vec::new())),
            Some(Some(release_id)) => self.get_track_ids_for_release(&release_id).await.map(Some),
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_album(&self, album: &DbAlbum) -> Result<(), DbError> {
        let album = album.clone();
        self.store.call_sql(move |sql| insert_album_row(&sql, &album)).await
    }
    /// Find album by ID. Caller-provided ID — may not exist.
    pub async fn find_album_by_id(&self, album_id: &str) -> Result<Option<DbAlbum>, DbError> {
        let album_id = album_id.to_string();
        self.store.read(move |sql| find_album_by_id_on(&sql, &album_id)).await
    }

    /// Follow `DbRelease.album_id` → `DbAlbum`. FK navigation — the row must
    /// exist. See the method conventions above.
    pub async fn get_album_for_release(&self, release: &DbRelease) -> Result<DbAlbum, DbError> {
        let album_id = release.album_id.clone();
        self.store
            .read(move |sql| {
                sql.query_row(
                    r#"
                    SELECT
                        id, title, year,
                        is_compilation,
                        created_at
                    FROM albums
                    WHERE id = ?
                    "#,
                    params![album_id],
                    row_to_album,
                )
                .map_err(DbError::from)
            })
            .await
    }

    /// Find album_id for a release. Caller-provided ID — may not exist.
    pub async fn find_album_id_for_release(
        &self,
        release_id: &str,
    ) -> Result<Option<String>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                sql.query_row(
                    "SELECT album_id FROM releases WHERE id = ?",
                    params![release_id],
                    |row| row.get::<_, String>("album_id"),
                )
                .optional()
                .map_err(DbError::from)
            })
            .await
    }

    /// Delete every release of `album_id` in one write, each as `releases`
    /// planned it. Refuse if the album's releases or any of theirs changed
    /// since planning; each coven file of theirs goes with its last
    /// reference.
    pub async fn delete_album_with_cleanup(
        &self,
        album_id: &str,
        releases: Vec<ReleaseDeletion>,
    ) -> Result<(), DeletionError> {
        let album_id = album_id.to_string();
        self.store
            .write(move |sql| {
                let mut current: Vec<String> = sql.query(
                    "SELECT id FROM releases WHERE album_id = ? ORDER BY id",
                    params![album_id],
                    |row| row.get(0),
                )?;
                current.sort();
                let mut planned: Vec<String> = releases
                    .iter()
                    .filter(|release| release.album_id() == album_id)
                    .map(|release| release.release_id().to_string())
                    .collect();
                planned.sort();
                if current != planned || planned.len() != releases.len() {
                    return Err(DeletionError::PlanChanged(Deleted::Album(album_id)));
                }
                for release in &releases {
                    release.apply_on(&sql)?;
                }
                // The releases go; the album row stays, empty, for the reason in
                // `vacate_album_on`.
                sql.execute("DELETE FROM releases WHERE album_id = ?", params![album_id])?;
                Ok(())
            })
            .await
    }

    pub async fn set_album_primary_release(
        &self,
        album_id: &str,
        primary_release_id: &str,
    ) -> Result<(), PrimaryReleaseError> {
        let (album, release) = (album_id.to_string(), primary_release_id.to_string());
        let set = self
            .store
            .call_sql(move |sql| {
                // The release is ordered below the album's lowest, so it is the
                // album's primary on every device that applies this write. One
                // already primary stays where it is.
                let current: Option<String> = sql
                    .query_row(
                        &format!(
                            "SELECT {} FROM albums a WHERE a.id = ?",
                            album_primary_release_sql()
                        ),
                        params![album],
                        |row| row.get(0),
                    )
                    .optional()?
                    .flatten();
                if current.as_deref() == Some(release.as_str()) {
                    return Ok(true);
                }
                let moved = sql.execute(
                    "UPDATE releases SET album_order = \
                     (SELECT MIN(album_order) - 1 FROM releases WHERE album_id = ?1) \
                 WHERE id = ?2 AND album_id = ?1",
                    params![album, release],
                )?;
                Ok(moved == 1)
            })
            .await?;
        match set {
            true => Ok(()),
            false => Err(PrimaryReleaseError::NotAlbumRelease {
                album_id: album_id.to_string(),
                release_id: primary_release_id.to_string(),
            }),
        }
    }
}
