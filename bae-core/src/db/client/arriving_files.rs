//! The rows whose files another device may still be uploading, and the
//! releases and tracks those files belong to.
use super::*;
use sha2::Digest;

/// A synced table whose rows carry a file (`crate::sync::synced_tables`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FileTable {
    /// A release's cover, keyed by the release.
    Covers,
    /// One of a release's audio or companion files.
    ReleaseFiles,
    /// An artist's picture, keyed by the artist.
    ArtistImages,
    /// A person's avatar, keyed by the person.
    People,
}

impl FileTable {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Covers => "covers",
            Self::ReleaseFiles => "release_files",
            Self::ArtistImages => "artist_images",
            Self::People => "people",
        }
    }
}

/// A row that carries an attached file, and a digest of the reference coven
/// wrote for it: a new attachment changes the digest even when the row and
/// its content stay the same. The reference holds the file's key, so only
/// its digest is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachedFileRow {
    pub(crate) table: FileTable,
    pub(crate) key: String,
    pub(crate) attachment: [u8; 32],
}

/// The release and album a release id names, for an arrival's line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArrivingReleaseRow {
    pub(crate) release_id: String,
    pub(crate) album_id: String,
    pub(crate) album_title: String,
}

/// What a set of arriving release files and covers belong to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArrivingReleaseRows {
    pub(crate) releases: Vec<ArrivingReleaseRow>,
    /// Each release file id with its release.
    pub(crate) file_releases: Vec<(String, String)>,
    /// Each release file id with a track whose audio it holds.
    pub(crate) file_tracks: Vec<(String, String)>,
}

const ATTACHED_FILE_ROWS: &str = "\
    SELECT 0, id, location FROM covers WHERE location IS NOT NULL \
    UNION ALL SELECT 1, id, location FROM release_files WHERE location IS NOT NULL \
    UNION ALL SELECT 2, id, location FROM artist_images WHERE location IS NOT NULL \
    UNION ALL SELECT 3, id, location FROM people WHERE location IS NOT NULL";

fn attached_file_rows_on(sql: &SqlReadContext<'_>) -> Result<Vec<AttachedFileRow>, DbError> {
    Ok(sql.query(ATTACHED_FILE_ROWS, [], |row| {
        let table = match row.get::<_, i64>(0)? {
            0 => FileTable::Covers,
            1 => FileTable::ReleaseFiles,
            2 => FileTable::ArtistImages,
            3 => FileTable::People,
            other => unreachable!("the query names four tables, not {other}"),
        };
        let location: String = row.get(2)?;
        Ok(AttachedFileRow {
            table,
            key: row.get(1)?,
            attachment: sha2::Sha256::digest(location.as_bytes()).into(),
        })
    })?)
}

fn arriving_release_rows_on(
    sql: &SqlReadContext<'_>,
    release_ids: &[String],
    file_ids: &[String],
) -> Result<ArrivingReleaseRows, DbError> {
    let mut file_releases = Vec::new();
    let mut file_tracks = Vec::new();
    for chunk in file_ids.chunks(SQL_MAX_IN_VARS) {
        let placeholders = in_clause_placeholders(chunk.len());
        file_releases.extend(sql.query(
            &format!("SELECT id, release_id FROM release_files WHERE id IN ({placeholders})"),
            rusqlite::params_from_iter(chunk.iter()),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?);
        file_tracks.extend(sql.query(
            &format!(
                "SELECT DISTINCT segment.file_id, format.track_id \
                 FROM audio_format_segments segment \
                 JOIN audio_formats format ON format.id = segment.audio_format_id \
                 WHERE segment.file_id IN ({placeholders})"
            ),
            rusqlite::params_from_iter(chunk.iter()),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?);
    }
    let named: BTreeSet<&String> = release_ids
        .iter()
        .chain(file_releases.iter().map(|(_, release)| release))
        .collect();
    let named: Vec<&String> = named.into_iter().collect();
    let mut releases = Vec::with_capacity(named.len());
    for chunk in named.chunks(SQL_MAX_IN_VARS) {
        releases.extend(sql.query(
            &format!(
                "SELECT release.id, album.id, album.title FROM releases release \
                 JOIN albums album ON album.id = release.album_id \
                 WHERE release.id IN ({})",
                in_clause_placeholders(chunk.len())
            ),
            rusqlite::params_from_iter(chunk.iter()),
            |row| {
                Ok(ArrivingReleaseRow {
                    release_id: row.get(0)?,
                    album_id: row.get(1)?,
                    album_title: row.get(2)?,
                })
            },
        )?);
    }
    Ok(ArrivingReleaseRows {
        releases,
        file_releases,
        file_tracks,
    })
}

impl Database {
    /// Every row of the library whose file is attached, live: the rows a
    /// file still arriving from another device can belong to.
    pub(crate) fn subscribe_attached_file_rows(&self) -> coven::LiveQuery<Vec<AttachedFileRow>> {
        self.inner
            .handle
            .subscribe(|sql| attached_file_rows_on(&sql).map_err(CovenError::from))
    }

    /// The releases, albums and tracks these covers (keyed by release) and
    /// release files belong to, in one read.
    pub(crate) async fn arriving_release_rows(
        &self,
        release_ids: Vec<String>,
        file_ids: Vec<String>,
    ) -> Result<ArrivingReleaseRows, DbError> {
        self.read(move |sql| arriving_release_rows_on(&sql, &release_ids, &file_ids))
            .await
    }
}
