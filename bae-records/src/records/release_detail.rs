//! A release as its detail reads it: its tracks by side with their credits
//! and works, its credits by role, its files with the lists among them, its
//! records, the other device its audio is on, and whether it
//! is pinned, stored, or queued to upload.

use super::files::cover_version_sql;
use super::query::*;
use super::read::*;
use super::release_credits::{credit_rows_column, release_credits_json_sql};
use super::release_tracks::{total_duration_sql, track_groups_column, track_groups_json_sql};
use super::*;

/// What a track's stars are when no one's are asked for: none.
pub const NO_STARS: &str = "NULL";

/// The releases a read of their detail covers.
#[derive(Debug, Clone, Copy)]
pub enum ReleaseSet<'a> {
    /// One release, by its id.
    One(&'a str),
    /// Every release of an album, by the album's id, in the album's order.
    OfAlbum(&'a str),
}

impl ReleaseSet<'_> {
    /// The condition and the order of the set over a row `rel` of `releases`,
    /// binding the id as `?1`.
    fn condition(&self) -> &'static str {
        match self {
            Self::One(_) => "rel.id = ?1 ORDER BY rel.id",
            Self::OfAlbum(_) => "rel.album_id = ?1 ORDER BY rel.album_order, rel.id",
        }
    }

    fn id(&self) -> &str {
        match self {
            Self::One(id) | Self::OfAlbum(id) => id,
        }
    }
}

/// The release `release_id`'s rows as its detail reads them, beside the
/// `extra_columns` (SQL over the release `rel`, empty for none) that `extra` reads; `None`
/// when the release does not exist. See [`release_details_on`].
pub fn release_detail_rows_on<X>(
    sql: &impl coven::SqlRead,
    release_id: &str,
    my_stars: &str,
    extra_columns: &str,
    extra: fn(&Row<'_>) -> rusqlite::Result<X>,
) -> Result<Option<(DbReleaseDetailRows, X)>, DbError> {
    Ok(release_details_on(sql, ReleaseSet::One(release_id), my_stars, extra_columns, extra)?
        .into_iter()
        .next())
}

/// The rows of the detail of each release of `set`, in one statement: each
/// release a row whose columns hold its tracks in groups (each with the stars
/// `my_stars` gives it, an SQL expression over the track `track`, or
/// [`NO_STARS`]), its credits, its files, records and the other
/// device its audio is on, and whether it is pinned, stored, or queued to upload. Beside each,
/// `extra_columns`, further columns of the release `rel` that `extra` reads.
pub fn release_details_on<X>(
    sql: &impl coven::SqlRead,
    set: ReleaseSet<'_>,
    my_stars: &str,
    extra_columns: &str,
    extra: fn(&Row<'_>) -> rusqlite::Result<X>,
) -> Result<Vec<(DbReleaseDetailRows, X)>, DbError> {
    let statement = format!(
        "SELECT rel.*, {labels} AS labels, {barcodes} AS barcodes, {media} AS media, \
                {total_duration} AS total_duration_ms, \
                {track_groups} AS track_groups, \
                {credits} AS credits, \
                {performs_works} AS performs_works, \
                {files} AS files, \
                {records} AS records, \
                EXISTS (SELECT 1 FROM release_records rr WHERE rr.release_id = rel.id) \
                    AS has_records, \
                {elsewhere} AS elsewhere, \
                COALESCE({RELEASE_STORAGE_PINNED}, 0) AS pinned, \
                COALESCE(storage.stored = 1, 0) AS remote, \
                {upload_columns}, \
                {cover_version} AS cover_version{extra_columns} \
         FROM releases rel \
         LEFT JOIN release_storage storage ON storage.release_id = rel.id \
         LEFT JOIN ({sending}) sending ON sending.release_id = rel.id \
         WHERE {condition}",
        labels = super::pressing_columns::release_labels_sql("rel.id"),
        barcodes = super::pressing_columns::release_barcodes_sql("rel.id"),
        media = super::pressing_columns::release_media_sql("rel.id"),
        total_duration = total_duration_sql("rel.id"),
        track_groups = track_groups_json_sql("rel.id", my_stars),
        credits = release_credits_json_sql("rel.id"),
        performs_works = performs_works_sql("rel.id"),
        files = release_files_json_sql("rel.id"),
        records = release_records_json_sql("rel.id"),
        elsewhere = ELSEWHERE_OF_RELEASE,
        upload_columns = super::queued_uploads::RELEASE_UPLOAD_COLUMNS,
        cover_version = cover_version_sql("rel.id"),
        extra_columns =
            if extra_columns.is_empty() { String::new() } else { format!(", {extra_columns}") },
        sending = super::queued_uploads::RELEASES_SENDING,
        condition = set.condition(),
    );
    Ok(sql.query(&statement, params![set.id()], |row| {
        let detail = DbReleaseDetailRows {
            release: row_to_release(row)?,
            total_duration_ms: bae_store::read::count_column(row, "total_duration_ms")?,
            track_groups: track_groups_column(row, "track_groups")?,
            credits: credit_rows_column(row, "credits")?,
            performs_works: row.get("performs_works")?,
            files: release_files_column(row, "files")?,
            records: records_column(row, "records")?,
            has_records: row.get("has_records")?,
            elsewhere: elsewhere_in_row(row, "elsewhere")?,
            pinned: row.get("pinned")?,
            remote: row.get("remote")?,
            upload: super::queued_uploads::release_upload_in_row(row)?,
            cover_version: row.get("cover_version")?,
        };
        Ok((detail, extra(row)?))
    })?)
}

/// A release's rows as [`release_details_on`] read them.
#[derive(Clone, PartialEq)]
pub struct DbReleaseDetailRows {
    release: DbRelease,
    total_duration_ms: u64,
    track_groups: Vec<DbTrackGroup>,
    credits: Vec<DbCreditRow>,
    performs_works: bool,
    files: DbReleaseFiles,
    records: Vec<bae_catalogs::records::ReleaseRecord>,
    has_records: bool,
    elsewhere: Option<OtherDevice>,
    pinned: bool,
    remote: bool,
    upload: Option<DbReleaseUpload>,
    cover_version: Option<String>,
}

impl DbReleaseDetailRows {
    /// The release the rows are of.
    pub fn release(&self) -> &DbRelease {
        &self.release
    }

    /// The release's detail, one entry per track.
    pub fn process(self) -> DbReleaseDetail {
        DbReleaseDetail {
            release: self.release,
            total_duration_ms: self.total_duration_ms,
            track_groups: self.track_groups,
            credits: self.credits,
            performs_works: self.performs_works,
            files: self.files,
            records: self.records,
            has_records: self.has_records,
            elsewhere: self.elsewhere,
            pinned: self.pinned,
            remote: self.remote,
            upload: self.upload,
            cover_version: self.cover_version,
        }
    }
}

/// Whether any track of the release `release` (an SQL expression naming its
/// id) performs a work.
fn performs_works_sql(release: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM track_works tw JOIN tracks t ON t.id = tw.track_id \
         WHERE t.release_id = {release})"
    )
}

/// The release `release` (an SQL expression naming its id)'s files, and among
/// them the images, the images a cover can be made from, and the files no
/// track plays from, each in the release's file order, and the files counted
/// and their sizes added, as one JSON object for [`release_files_column`].
fn release_files_json_sql(release: &str) -> String {
    let covers: Vec<String> = ContentType::SUPPORTED_COVERS
        .iter()
        .map(|content_type| format!("'{}'", content_type.as_str()))
        .collect();
    let list = |condition: &str| {
        format!(
            "json_group_array(json(listed.file_json) \
                 ORDER BY listed.original_filename COLLATE natural_order, listed.id) \
             FILTER (WHERE {condition})"
        )
    };
    format!(
        "(SELECT json_object( \
              'files', json({files}), 'images', json({images}), 'covers', json({covers}), \
              'others', json({others}), \
              'has_others', COALESCE(SUM(listed.source_audio_layout IS NULL), 0) > 0, \
              'count', COUNT(*), 'total_size', COALESCE(SUM(listed.size), 0)) \
          FROM (SELECT f.id, f.original_filename, f.content_type, f.source_audio_layout, \
                       coven.size AS size, {file_json} AS file_json \
                FROM release_files AS f \
                LEFT JOIN coven_files AS coven ON coven.id = f.file \
                WHERE f.release_id = {release}) AS listed)",
        files = list("1"),
        images = list("listed.content_type GLOB 'image/*'"),
        covers = list(&format!("listed.content_type IN ({})", covers.join(", "))),
        others = list("listed.source_audio_layout IS NULL"),
        file_json = file_json_sql("coven.size", "coven.hash IS NOT NULL"),
        release = release,
    )
}

/// The release's files as [`release_files_json_sql`] read them into `column`.
fn release_files_column(row: &Row<'_>, column: &str) -> rusqlite::Result<DbReleaseFiles> {
    #[derive(serde::Deserialize)]
    struct Stored {
        files: Vec<FileColumns>,
        images: Vec<FileColumns>,
        covers: Vec<FileColumns>,
        others: Vec<FileColumns>,
        has_others: i64,
        count: i64,
        total_size: u64,
    }
    let stored: Stored = bae_store::read::json_column(row, column)?;
    let files = |columns: Vec<FileColumns>| {
        columns
            .into_iter()
            .map(|file| {
                file_of(file).map_err(|(field, reason)| {
                    column_conversion_error(row, column, format!("{field}: {reason}"))
                })
            })
            .collect::<rusqlite::Result<Vec<_>>>()
    };
    Ok(DbReleaseFiles {
        files: files(stored.files)?,
        images: files(stored.images)?,
        covers: files(stored.covers)?,
        others: files(stored.others)?,
        has_others: stored.has_others != 0,
        count: stored.count,
        total_size: stored.total_size,
    })
}

/// A JSON array of the name and state of the device that alone holds the
/// audio file of the release `rel` with the lowest file id among the files
/// another device alone holds, and whether a member removed that device's
/// person from the library. The column holds the JSON text `null` when no
/// other device alone holds an audio file of the release.
const ELSEWHERE_OF_RELEASE: &str = "COALESCE((SELECT json_array(e.device_name, e.device_state, \
     e.member_removed) FROM release_audio_elsewhere e WHERE e.release_id = rel.id \
     ORDER BY e.file_id LIMIT 1), 'null')";

/// The other device a release's audio is on, from `column` of a row that
/// holds [`ELSEWHERE_OF_RELEASE`]. It returns `None` when no other device
/// alone holds an audio file of the release.
fn elsewhere_in_row(row: &Row<'_>, column: &str) -> rusqlite::Result<Option<OtherDevice>> {
    let Some((name, state, member_removed)) =
        bae_store::read::json_column::<Option<(String, String, i64)>, _>(row, column)?
    else {
        return Ok(None);
    };
    other_device(name, &state, member_removed != 0).map(Some).ok_or_else(|| {
        bae_store::read::column_conversion_error(
            row,
            column,
            format!("{state:?} is no state coven writes for a device"),
        )
    })
}

/// Why bae could not name the other device that holds a release file.
#[derive(Debug, thiserror::Error)]
pub enum OtherDeviceError {
    /// No other device alone holds the audio file. The file reached storage,
    /// this device holds it, or the file left the library, after the read that
    /// found it on another device.
    #[error("release file {0} is not an audio file on another device only")]
    NotOnAnotherDevice(String),
    /// `coven_devices` records a state for the device that coven does not write.
    #[error(
        "the device that holds release file {file} has a state coven does not write: {state:?}"
    )]
    UnknownState { file: String, state: String },
    #[error(transparent)]
    Database(#[from] DbError),
}

impl Records {
    /// The other device that alone holds the audio file `file_id`, as the
    /// view `release_audio_elsewhere` reads it from coven's tables. The read fails naming the
    /// file when no other device alone holds it.
    pub async fn release_file_other_device(
        &self,
        file_id: &str,
    ) -> Result<OtherDevice, OtherDeviceError> {
        let file = file_id.to_string();
        let held = self
            .store
            .read(move |sql| {
                Ok(sql
                    .query(
                        "SELECT device_name, device_state, member_removed \
                         FROM release_audio_elsewhere WHERE file_id = ?1",
                        params![file],
                        |row| {
                            Ok((
                                row.get::<_, String>("device_name")?,
                                row.get::<_, String>("device_state")?,
                                row.get::<_, bool>("member_removed")?,
                            ))
                        },
                    )?
                    .pop())
            })
            .await?;
        let (name, state, member_removed) =
            held.ok_or_else(|| OtherDeviceError::NotOnAnotherDevice(file_id.into()))?;
        other_device(name, &state, member_removed)
            .ok_or_else(|| OtherDeviceError::UnknownState { file: file_id.into(), state })
    }
}

/// Another device named `name`, as coven's registration `state` of it reads:
/// removed when its registration says so or its person was removed from the
/// library (`member_removed`), replaced when it was reset. `None` for a state
/// coven doesn't write.
fn other_device(name: String, state: &str, member_removed: bool) -> Option<crate::OtherDevice> {
    use crate::OtherDevice;
    use bae_store::FormerDeviceState;
    if let Some(state) = FormerDeviceState::of_state(state) {
        return Some(OtherDevice::Former { name, state });
    }
    Some(match state {
        "active" if member_removed => {
            OtherDevice::Former { name, state: FormerDeviceState::Removed }
        }
        "active" => OtherDevice::Active { name },
        _ => return None,
    })
}

#[cfg(test)]
#[path = "release_detail_tests.rs"]
mod tests;
