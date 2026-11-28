use super::*;
use bae_store::WriteStatements as _;

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

coven::write!(
    DeleteAlbumReleases(album_id: &'p str),
    "DELETE FROM releases WHERE album_id = :album_id"
);

coven::write!(
    MakePrimaryRelease(album_id: &'p str, release_id: &'p str),
    r#"
    UPDATE releases
    SET album_order = (SELECT MIN(album_order) - 1 FROM releases WHERE album_id = :album_id)
    WHERE id = :release_id AND album_id = :album_id
"#
);

// The album the release `release` is in; no row when the library holds no
// such release.
coven::read!(
    writer AlbumOfRelease(release: String),
    r#"
    SELECT release.album_id
    FROM releases AS release
    WHERE release.id = :release
"#
);

// The ids of the album `album`'s releases, in id order.
coven::read!(
    writer ReleaseIdsOfAlbum(album: String),
    r#"
    SELECT release.id
    FROM releases AS release
    WHERE release.album_id = :album
    ORDER BY release.id
"#
);

// The album `album`'s primary release
// ([`AlbumPrimaryRelease`](super::fragments::AlbumPrimaryRelease)), none when
// it has no release; no row when the library holds no such album.
coven::read!(
    writer AlbumPrimaryReleaseId(album: String),
    r#"
    SELECT @crate::records::fragments::AlbumPrimaryRelease(parent: album) AS lowest
    FROM albums AS album
    WHERE album.id = :album
"#
);

impl Records {
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
        let statement = OneAlbumParams { album: album_id };
        self.store.read_row(statement).await?.album.into_album()
    }

    /// Find album_id for a release. Caller-provided ID — may not exist.
    pub async fn find_album_id_for_release(
        &self,
        release_id: &str,
    ) -> Result<Option<String>, DbError> {
        let release_id = release_id.to_string();
        let statement = AlbumOfReleaseParams { release: release_id };
        let row =
            self.store.read(move |sql| bae_store::statement_optional(&sql, &statement)).await?;
        Ok(row.map(|row| row.album_id))
    }

    /// Delete every release of `album_id` in one write, each as `releases`
    /// planned it. Refuse if the album's releases or any of theirs changed
    /// since planning; each coven file of theirs goes with its last
    /// reference, and the album with its last release.
    pub async fn delete_album_with_cleanup(
        &self,
        album_id: &str,
        releases: Vec<ReleaseDeletion>,
    ) -> Result<(), DeletionError> {
        let album_id = album_id.to_string();
        self.store
            .write(move |sql| {
                let release_ids: Vec<&str> =
                    releases.iter().map(|release| release.release_id()).collect();
                let emptiable = Emptiable::of_releases(&sql, &release_ids)?;
                let current: Vec<String> = bae_store::statement_rows(
                    &sql,
                    &ReleaseIdsOfAlbumParams { album: album_id.clone() },
                )?
                .into_iter()
                .map(|row| row.id)
                .collect();
                let mut planned: Vec<String> = releases
                    .iter()
                    .filter(|release| release.album_id() == album_id)
                    .map(|release| release.release_id().to_string())
                    .collect();
                planned.sort();
                if current != planned || planned.len() != releases.len() {
                    return Err(WriteError::Refused(DeletionRefusal::PlanChanged(Deleted::Album(
                        album_id,
                    ))));
                }
                for release in &releases {
                    release.apply_on(&sql)?;
                }
                sql.run(&DeleteAlbumReleasesParams { album_id: &album_id })?;
                Ok(emptiable.remove_emptied(&sql)?)
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
                let current = bae_store::statement_optional(
                    &sql,
                    &AlbumPrimaryReleaseIdParams { album: album.clone() },
                )?
                .and_then(|row| row.lowest.map(|lowest| lowest.release_id));
                if current.as_deref() == Some(release.as_str()) {
                    return Ok(true);
                }
                let moved =
                    sql.run(&MakePrimaryReleaseParams { album_id: &album, release_id: &release })?;
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
