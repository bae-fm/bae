//! A release as its detail reads it: its tracks with their credits, its
//! files, records and mediums, the audio this device holds on one device
//! only, and whether it is pinned, stored, or queued to upload.

use super::files::cover_version_sql;
use super::query::*;
use super::read::*;
use super::*;
use bae_util::content_type::ContentType;

/// The release's rows as its detail reads them: its tracks with their
/// credits, its files, records, mediums and audio on one device only, and
/// whether it is pinned, stored, or queued to upload.
pub fn release_detail_rows_on(
    sql: &SqlReadContext<'_>,
    release: DbRelease,
) -> Result<DbReleaseDetailRows, DbError> {
    let tracks = get_tracks_with_artists_for_release_on(sql, &release.id)?;
    let files = get_files_for_release_on(sql, &release.id)?;
    let records = get_release_records_on(sql, &release.id)?;
    let unstored_audio = unstored_audio_on(sql, &release.id)?;
    let mediums = release_mediums_on(sql, &release.id)?;
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
        mediums,
        tracks,
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
    mediums: DbReleaseMediums,
    tracks: Vec<DbTrackWithArtists>,
    files: Vec<DbFile>,
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
            mediums: self.mediums,
            tracks: self.tracks,
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

/// The release's tracks in playing order, each with its own credits and the
/// credits it shows.
fn get_tracks_with_artists_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbTrackWithArtists>, DbError> {
    let query = format!(
        "SELECT
            track.id AS track_id,
            track.release_id AS track_release_id,
            track.title AS track_title,
            track.medium_id AS track_medium_id,
            track.side AS track_side,
            track.track_number AS track_track_number,
            track.duration_ms AS track_duration_ms,
            track.created_at AS track_created_at,
            {own} AS own_credits,
            {shown} AS shown_credits
         FROM tracks track
         JOIN releases r ON r.id = track.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE track.release_id = ?
         ORDER BY track.position, track.id",
        own = track_own_credits_json_sql("track"),
        shown = track_credits_json_sql("track"),
    );
    Ok(sql.query(&query, params![release_id], |row| {
        Ok(DbTrackWithArtists {
            track: row_to_track_with_prefix(row, "track_")?,
            artists: credits_column(row, "own_credits")?,
            shown_artists: credits_column(row, "shown_credits")?,
        })
    })?)
}

/// What coven records this device holding for each of a release's audio
/// files on one device only: its original, when `coven_user_files`, which
/// lists exactly this device's originals, lists it; otherwise the device it
/// is on, from `coven_devices`, and whether that device or its person was
/// removed from the library. The disk is not asked.
fn unstored_audio_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbUnstoredAudio>, DbError> {
    unstored_rows_on(sql, "f.release_id = ?", release_id)?
        .into_iter()
        .filter(|row| ContentType::from_mime(&row.content_type).is_audio())
        .map(UnstoredRow::into_unstored_audio)
        .collect()
}

/// The release files `filter` picks, binding `value`, that are on one device
/// only, with what coven records of that device.
fn unstored_rows_on(
    sql: &SqlReadContext<'_>,
    filter: &str,
    value: &str,
) -> Result<Vec<UnstoredRow>, DbError> {
    Ok(sql.query(
        &format!(
            "SELECT f.content_type, f.file, u.file IS NOT NULL, \
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
                content_type: row.get(0)?,
                file: row.get(1)?,
                original_here: row.get(2)?,
                is_self: row.get(3)?,
                name: row.get(4)?,
                state: row.get(5)?,
                member_removed: row.get(6)?,
            })
        },
    )?)
}

impl Database {
    /// The other device release file `file_id` is on, for a read of it that
    /// found it on one device only, not this one. A file coven records here,
    /// or on no one device, fails the read naming the file.
    pub async fn release_file_other_device(&self, file_id: &str) -> Result<OtherDevice, DbError> {
        let file_id = file_id.to_string();
        self.inner
            .store
            .read(move |sql| {
                let mut rows = unstored_rows_on(&sql, "f.id = ?", &file_id)?;
                let (Some(row), None) = (rows.pop(), rows.pop()) else {
                    return Err(DbError::Message(format!(
                        "coven lists release file {file_id} on no one device"
                    )));
                };
                match row.into_unstored_audio()? {
                    DbUnstoredAudio::Elsewhere(device) => Ok(device),
                    DbUnstoredAudio::Original => Err(DbError::Message(format!(
                        "release file {file_id} read as on another device, with its original here"
                    ))),
                }
            })
            .await
    }
}

/// One release file on one device only, as [`unstored_audio_on`] reads it.
struct UnstoredRow {
    content_type: String,
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
        let broken = |what: &str| {
            DbError::Message(format!(
                "coven lists release file {} on one device only, {what}",
                self.file
            ))
        };
        // This device has no registration until the library connects to
        // storage, so its own file can be on a device `coven_devices` doesn't
        // list; another device's never is.
        match (self.original_here, self.is_self) {
            (true, Some(false)) => Err(broken("on another device, with an original recorded here")),
            (true, None | Some(true)) => Ok(DbUnstoredAudio::Original),
            (false, Some(false)) => Ok(DbUnstoredAudio::Elsewhere(self.other_device()?)),
            (false, Some(true)) => Err(broken("on this device, with no original recorded")),
            (false, None) => Err(broken(
                "on another device it has no registration of, or on this device with no original recorded",
            )),
        }
    }

    /// The device the file is on, which is not this one.
    fn other_device(&self) -> Result<crate::OtherDevice, DbError> {
        let (Some(name), Some(state), Some(member_removed)) =
            (self.name.clone(), self.state.as_deref(), self.member_removed)
        else {
            return Err(DbError::Message(format!(
                "coven_devices lists the device of release file {} as {:?}, {:?}, {:?}",
                self.file, self.name, self.state, self.member_removed
            )));
        };
        other_device(name, state, member_removed).ok_or_else(|| {
            DbError::Message(format!(
                "coven_devices lists the device of release file {} as {state}",
                self.file
            ))
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
    Some(match state {
        "replaced" => OtherDevice::Former { name, state: FormerDeviceState::Replaced },
        "removed" => OtherDevice::Former { name, state: FormerDeviceState::Removed },
        "active" if member_removed => {
            OtherDevice::Former { name, state: FormerDeviceState::Removed }
        }
        "active" => OtherDevice::Active { name },
        _ => return None,
    })
}

/// What this device holds for each of `release_id`'s audio files on one
/// device only, as a release's detail reads it, for a row mapper reading a
/// release's summary: its failure is carried as the mapper's error.
pub fn unstored_audio_in_row(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> rusqlite::Result<Vec<DbUnstoredAudio>> {
    unstored_audio_on(sql, release_id)
        .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))
}

#[cfg(test)]
#[path = "release_detail_tests.rs"]
mod tests;
