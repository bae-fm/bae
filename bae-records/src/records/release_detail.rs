//! A release as its detail reads it: its tracks by side with their credits
//! and works, its credits by role, its files with the lists among them, its
//! records, the audio this device holds on one device only, and whether it
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
/// [`NO_STARS`]), its credits, its files, records and audio on one device
/// only, and whether it is pinned, stored, or queued to upload. Beside each,
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
                {files} AS files, \
                {records} AS records, \
                {unstored_audio} AS unstored_audio, \
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
        files = release_files_json_sql("rel.id"),
        records = release_records_json_sql("rel.id"),
        unstored_audio = unstored_audio_json_sql("rel.id"),
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
            files: release_files_column(row, "files")?,
            records: records_column(row, "records")?,
            unstored_audio: unstored_audio_column(row, "unstored_audio")?,
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
    files: DbReleaseFiles,
    records: Vec<bae_catalogs::records::ReleaseRecord>,
    unstored_audio: Vec<DbUnstoredAudio>,
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
            files: self.files,
            records: self.records,
            unstored_audio: self.unstored_audio,
            pinned: self.pinned,
            remote: self.remote,
            upload: self.upload,
            cover_version: self.cover_version,
        }
    }
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
        count: stored.count,
        total_size: stored.total_size,
    })
}

/// The release files, the registrations of the devices they are on and the
/// members those belong to, as [`unstored_audio_on`] and
/// [`release_file_other_device`](Records::release_file_other_device) read
/// them: `f`, `coven`, the file's original `u` on this device when
/// `coven_user_files` lists it, its device `d` and that device's member `m`.
const UNSTORED_FILES: &str = "FROM release_files f JOIN coven_files coven ON coven.id = f.file \
     LEFT JOIN coven_user_files u ON u.file = f.file \
     LEFT JOIN coven_devices d ON d.device = coven_file_device(coven.coordinates) \
     LEFT JOIN coven_members m ON m.member = d.member";

/// The columns of one release file on one device only, as an `UnstoredRow`
/// reads them.
const UNSTORED_COLUMNS: &str = "f.file, u.file IS NOT NULL AS original_here, \
     d.is_self, d.name, d.state, m.removed AS member_removed";

/// What coven records this device holding for each audio file of the release
/// `release_expr` (an SQL expression) that is on one device only, in file
/// order, as a JSON array for [`unstored_audio_column`]: its original, when
/// `coven_user_files`, which lists exactly this device's originals, lists it;
/// otherwise the device it is on, from `coven_devices`, and whether that
/// device or its person was removed from the library. The disk is not asked.
pub fn unstored_audio_json_sql(release_expr: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
              'file', f.file, 'original_here', u.file IS NOT NULL, 'is_self', d.is_self, \
              'name', d.name, 'state', d.state, 'member_removed', m.removed) ORDER BY f.id) \
          {UNSTORED_FILES} \
          WHERE f.release_id = {release_expr} AND f.content_type GLOB 'audio/*' \
              AND coven_file_device(coven.coordinates) IS NOT NULL)"
    )
}

/// Whether an audio file of the release `release_expr` (an SQL expression)
/// is on another device only: coven lists no original of it on this device
/// and a device that is not this one holds it. The release cannot play here
/// until that device moves the file to the cloud.
pub fn release_audio_elsewhere_sql(release_expr: &str) -> String {
    format!(
        "EXISTS (SELECT 1 {UNSTORED_FILES} \
                 WHERE f.release_id = {release_expr} AND f.content_type GLOB 'audio/*' \
                     AND coven_file_device(coven.coordinates) IS NOT NULL \
                     AND u.file IS NULL AND d.is_self = 0)"
    )
}

/// The audio [`unstored_audio_json_sql`] reads into `column`; SQLite writes
/// each flag in the JSON as 0 or 1.
pub fn unstored_audio_column(
    row: &Row<'_>,
    column: &str,
) -> rusqlite::Result<Vec<DbUnstoredAudio>> {
    #[derive(serde::Deserialize)]
    struct Stored {
        file: String,
        original_here: i64,
        is_self: Option<i64>,
        name: Option<String>,
        state: Option<String>,
        member_removed: Option<i64>,
    }
    let stored: Vec<Stored> = bae_store::read::json_column(row, column)?;
    stored
        .into_iter()
        .map(|stored| {
            UnstoredRow {
                file: stored.file,
                original_here: stored.original_here != 0,
                is_self: stored.is_self.map(|is_self| is_self != 0),
                name: stored.name,
                state: stored.state,
                member_removed: stored.member_removed.map(|removed| removed != 0),
            }
            .into_unstored_audio()
            .map_err(|error| {
                bae_store::read::column_conversion_error(row, column, error.to_string())
            })
        })
        .collect()
}

/// What coven records this device holding for each of a release's audio
/// files on one device only (see [`unstored_audio_json_sql`]).
pub fn unstored_audio_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Vec<DbUnstoredAudio>, DbError> {
    Ok(sql.query_row(
        &format!("SELECT {} AS unstored_audio", unstored_audio_json_sql("?1")),
        params![release_id],
        |row| unstored_audio_column(row, "unstored_audio"),
    )?)
}

/// The release file `file_id`, when it is on one device only, with what coven
/// records of that device.
fn unstored_row_of_file_on(
    sql: &impl coven::SqlRead,
    file_id: &str,
) -> Result<Vec<UnstoredRow>, DbError> {
    Ok(sql.query(
        &format!(
            "SELECT {UNSTORED_COLUMNS} {UNSTORED_FILES} \
             WHERE f.id = ?1 AND coven_file_device(coven.coordinates) IS NOT NULL"
        ),
        params![file_id],
        |row| {
            Ok(UnstoredRow {
                file: row.get("file")?,
                original_here: row.get("original_here")?,
                is_self: row.get("is_self")?,
                name: row.get("name")?,
                state: row.get("state")?,
                member_removed: row.get("member_removed")?,
            })
        },
    )?)
}

/// Why the other device a release file is on could not be named, for a
/// read of the file that found it on one device only, not this one.
#[derive(Debug, thiserror::Error)]
pub enum OtherDeviceError {
    /// Coven lists the file on no one device: it reached storage, or left
    /// the library, after the read that found it on one.
    #[error("release file {0} is on no one device")]
    NotOnOneDevice(String),
    /// Coven lists the file's original on this device.
    #[error("release file {0} has its original on this device")]
    OriginalHere(String),
    #[error(transparent)]
    Database(#[from] DbError),
}

impl Records {
    /// The other device release file `file_id` is on, for a read of it that
    /// found it on one device only, not this one. A file coven records here,
    /// or on no one device, fails the read naming the file.
    pub async fn release_file_other_device(
        &self,
        file_id: &str,
    ) -> Result<OtherDevice, OtherDeviceError> {
        let file = file_id.to_string();
        let audio = self
            .store
            .read(move |sql| {
                let mut rows = unstored_row_of_file_on(&sql, &file)?;
                match (rows.pop(), rows.pop()) {
                    (Some(row), None) => Ok(Some(row.into_unstored_audio()?)),
                    _ => Ok(None),
                }
            })
            .await?;
        match audio {
            Some(DbUnstoredAudio::Elsewhere(device)) => Ok(device),
            Some(DbUnstoredAudio::Original) => Err(OtherDeviceError::OriginalHere(file_id.into())),
            None => Err(OtherDeviceError::NotOnOneDevice(file_id.into())),
        }
    }
}

/// One release file on one device only, as [`unstored_audio_on`] reads it.
struct UnstoredRow {
    file: String,
    /// Whether `coven_user_files` lists an original of it on this device.
    original_here: bool,
    /// Whether the device the file is on is this one; `None` when
    /// `coven_devices` lists no such device, as for this device until the
    /// library connects to storage.
    is_self: Option<bool>,
    name: Option<String>,
    state: Option<String>,
    member_removed: Option<bool>,
}

impl UnstoredRow {
    /// Coven lists this device's own originals, keeps every device's
    /// registration for good, and a file is on another device only once that
    /// device, registered, wrote it: a file on this device with no original,
    /// an original for a file on another device, or another device with no
    /// registration is coven's own state broken, and the read fails naming
    /// the file.
    fn into_unstored_audio(self) -> Result<DbUnstoredAudio, DbError> {
        let broken = || {
            DbError::unreadable(
                format!(
                    "coven's place of release file {}: its original here, its device this one",
                    self.file
                ),
                (self.original_here, self.is_self),
            )
        };
        // This device has no registration until the library connects to
        // storage, so its own file can be on a device `coven_devices` doesn't
        // list; another device's never is.
        match (self.original_here, self.is_self) {
            (true, Some(false)) | (false, Some(true) | None) => Err(broken()),
            (true, None | Some(true)) => Ok(DbUnstoredAudio::Original),
            (false, Some(false)) => Ok(DbUnstoredAudio::Elsewhere(self.other_device()?)),
        }
    }

    /// The device the file is on, which is not this one.
    fn other_device(&self) -> Result<crate::OtherDevice, DbError> {
        let (Some(name), Some(state), Some(member_removed)) =
            (self.name.clone(), self.state.as_deref(), self.member_removed)
        else {
            return Err(DbError::unreadable(
                format!("coven_devices of the device release file {} is on", self.file),
                (&self.name, &self.state, self.member_removed),
            ));
        };
        other_device(name, state, member_removed).ok_or_else(|| {
            DbError::unreadable(
                format!("coven_devices.state of the device release file {} is on", self.file),
                state,
            )
        })
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
