//! A release as its detail reads it: its tracks by side with their credits
//! and works, its credits by role, its files with the lists among them, its
//! records, the audio this device holds on one device only, and whether it
//! is pinned, stored, or queued to upload.

use super::files::cover_version_sql;
use super::query::*;
use super::read::*;
use super::release_credits::release_credits_on;
use super::release_tracks::{release_total_duration_on, release_track_groups_on};
use super::*;

/// What a track's stars are when no one's are asked for: none.
pub const NO_STARS: &str = "NULL";

/// The release's rows as its detail reads them: its tracks in groups, each
/// with the stars `my_stars` gives it (an SQL expression over the track
/// `track`, or [`NO_STARS`]), its credits, its files, records and audio on
/// one device only, and whether it is pinned, stored, or queued to upload.
pub fn release_detail_rows_on(
    sql: &impl coven::SqlRead,
    release: DbRelease,
    my_stars: &str,
) -> Result<DbReleaseDetailRows, DbError> {
    let total_duration_ms = release_total_duration_on(sql, &release.id)?;
    let track_groups = release_track_groups_on(sql, &release.id, my_stars)?;
    let credits = release_credits_on(sql, &release.id)?;
    let files = release_files_on(sql, &release.id)?;
    let records = get_release_records_on(sql, &release.id)?;
    let unstored_audio = unstored_audio_on(sql, &release.id)?;
    let (pinned, remote) = sql.query_row(
        &format!(
            "SELECT EXISTS (SELECT 1 FROM release_storage storage \
                            WHERE storage.release_id = ?1 AND {RELEASE_STORAGE_PINNED}), \
                    EXISTS (SELECT 1 FROM release_storage storage \
                            WHERE storage.release_id = ?1 AND storage.stored = 1)"
        ),
        params![release.id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let cover_version = sql.query_row(
        &format!("SELECT {}", cover_version_sql("?1")),
        params![release.id],
        |row| row.get(0),
    )?;
    let upload = super::queued_uploads::release_upload_on(sql, &release.id)?;

    Ok(DbReleaseDetailRows {
        release,
        total_duration_ms,
        track_groups,
        credits,
        files,
        records,
        unstored_audio,
        pinned,
        remote,
        upload,
        cover_version,
    })
}

/// A release's rows as [`release_detail_rows_on`] read them.
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

/// The release's files, and among them the images, the images a cover can
/// be made from, and the files no track plays from, each a statement of its
/// own over the release's files in their order, and the files counted and
/// their sizes added.
fn release_files_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<DbReleaseFiles, DbError> {
    let among = |condition: &str| {
        sql.query(
            &format!(
                "SELECT {columns} FROM release_files AS f WHERE f.release_id = ?1 AND {condition} \
                 ORDER BY f.original_filename COLLATE natural_order, f.id",
                columns = release_file_columns!(),
            ),
            params![release_id],
            row_to_file,
        )
    };
    let covers: Vec<String> = ContentType::SUPPORTED_COVERS
        .iter()
        .map(|content_type| format!("'{}'", content_type.as_str()))
        .collect();
    let (count, total_size) = sql.query_row(
        "SELECT COUNT(*), COALESCE(SUM(coven.size), 0) \
         FROM release_files AS f JOIN coven_files AS coven ON coven.id = f.file \
         WHERE f.release_id = ?1",
        params![release_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(DbReleaseFiles {
        files: get_files_for_release_on(sql, release_id)?,
        images: among("f.content_type GLOB 'image/*'")?,
        covers: among(&format!("f.content_type IN ({})", covers.join(", ")))?,
        others: among("f.source_audio_layout IS NULL")?,
        count,
        total_size,
    })
}

/// What coven records this device holding for each of a release's audio
/// files on one device only: its original, when `coven_user_files`, which
/// lists exactly this device's originals, lists it; otherwise the device it
/// is on, from `coven_devices`, and whether that device or its person was
/// removed from the library. The disk is not asked.
pub fn unstored_audio_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Vec<DbUnstoredAudio>, DbError> {
    unstored_rows_on(sql, "f.release_id = ? AND f.content_type GLOB 'audio/*'", release_id)?
        .into_iter()
        .map(UnstoredRow::into_unstored_audio)
        .collect()
}

/// The release files `filter` picks, binding `value`, that are on one device
/// only, with what coven records of that device.
fn unstored_rows_on(
    sql: &impl coven::SqlRead,
    filter: &str,
    value: &str,
) -> Result<Vec<UnstoredRow>, DbError> {
    Ok(sql.query(
        &format!(
            "SELECT f.file, u.file IS NOT NULL, \
                    d.is_self, d.name, d.state, m.removed \
             FROM release_files f JOIN coven_files coven ON coven.id = f.file \
             LEFT JOIN coven_user_files u ON u.file = f.file \
             LEFT JOIN coven_devices d ON d.device = coven_file_device(coven.coordinates) \
             LEFT JOIN coven_members m ON m.member = d.member \
             WHERE {filter} AND coven_file_device(coven.coordinates) IS NOT NULL \
             ORDER BY f.id"
        ),
        params![value],
        |row| {
            Ok(UnstoredRow {
                file: row.get(0)?,
                original_here: row.get(1)?,
                is_self: row.get(2)?,
                name: row.get(3)?,
                state: row.get(4)?,
                member_removed: row.get(5)?,
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
                let mut rows = unstored_rows_on(&sql, "f.id = ?", &file)?;
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
