//! A release as its detail reads it: its tracks with their credits, its
//! files, records and mediums, the audio this device holds on one device
//! only, and whether it is pinned, stored, or queued to upload.

use super::files::cover_version_sql;
use super::query::*;
use super::read::*;
use super::*;
use crate::credits::LibraryCredit;
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
            "SELECT EXISTS (SELECT 1 FROM release_listing listing \
                            WHERE listing.release_id = ?1 AND {RELEASE_LISTING_PINNED}), \
                    EXISTS (SELECT 1 FROM release_listing listing \
                            WHERE listing.release_id = ?1 AND listing.stored = 1)"
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

/// A release's rows as [`release_detail_rows_on`] read them, before the
/// tracks' credit rows are folded into one entry per track, which runs after
/// the read has let its connection go.
#[derive(Clone, PartialEq)]
pub struct DbReleaseDetailRows {
    release: DbRelease,
    mediums: DbReleaseMediums,
    tracks: Vec<(DbTrack, Option<LibraryCredit>)>,
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
            tracks: process_tracks(self.tracks),
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

/// One row per (track, artist) pair, so a track with several artists repeats.
/// The rows arrive grouped by track and ordered by artist position, and the fold
/// in [`process_tracks`] rebuilds one entry per track from that run.
fn get_tracks_with_artists_for_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<(DbTrack, Option<LibraryCredit>)>, DbError> {
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
            {LIBRARY_CREDIT_COLUMNS}
         FROM tracks track
         LEFT JOIN track_artists credit ON credit.track_id = track.id
         LEFT JOIN artists artist ON artist.id = {}
         LEFT JOIN artist_credits credit_name
             ON credit_name.artist_id = credit.artist_id
            AND credit_name.name_key = credit.credit_key
         WHERE track.release_id = ?
         ORDER BY track.position, track.id, credit.position",
        shown_artist_id("credit.artist_id")
    );
    let joined = sql.query(&query, params![release_id], |row| {
        let track = row_to_track_with_prefix(row, "track_")?;
        let artist_id: Option<String> = row.get("artist_id")?;
        let credit = match artist_id {
            Some(_) => Some(row_to_library_credit(row)?),
            None => None,
        };
        Ok((track, credit))
    })?;

    Ok(joined)
}

fn process_tracks(joined: Vec<(DbTrack, Option<LibraryCredit>)>) -> Vec<DbTrackWithArtists> {
    let mut tracks: Vec<DbTrackWithArtists> = Vec::new();
    for (track, artist) in joined {
        if tracks.last().map(|last| last.track.id.as_str()) != Some(track.id.as_str()) {
            tracks.push(DbTrackWithArtists { track, artists: Vec::new() });
        }
        if let Some(credit) = artist {
            tracks.last_mut().expect("the row's track was just pushed").artists.push(credit);
        }
    }

    tracks
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
    let rows: Vec<UnstoredRow> = sql.query(
        "SELECT f.content_type, f.file, u.file IS NOT NULL, \
                d.is_self, d.name, d.state, m.removed \
         FROM release_files f JOIN coven_files coven ON coven.id = f.file \
         LEFT JOIN coven_user_files u ON u.file = f.file \
         LEFT JOIN coven_devices d ON d.device = coven_file_device(coven.coordinates) \
         LEFT JOIN coven_members m ON m.member = d.member \
         WHERE f.release_id = ? AND coven_file_device(coven.coordinates) IS NOT NULL \
         ORDER BY f.id",
        params![release_id],
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
    )?;
    rows.into_iter()
        .filter(|row| ContentType::from_mime(&row.content_type).is_audio())
        .map(UnstoredRow::into_unstored_audio)
        .collect()
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

    /// The device the file is on, which is not this one: removed when its
    /// registration says so or its person was removed from the library.
    fn other_device(&self) -> Result<crate::OtherDevice, DbError> {
        use crate::membership::FormerDeviceState;
        use crate::OtherDevice;
        let (Some(name), Some(state), Some(member_removed)) =
            (self.name.clone(), self.state.as_deref(), self.member_removed)
        else {
            return Err(DbError::Message(format!(
                "coven_devices lists the device of release file {} as {:?}, {:?}, {:?}",
                self.file, self.name, self.state, self.member_removed
            )));
        };
        Ok(match state {
            "replaced" => OtherDevice::Former { name, state: FormerDeviceState::Replaced },
            "removed" => OtherDevice::Former { name, state: FormerDeviceState::Removed },
            "active" if member_removed => {
                OtherDevice::Former { name, state: FormerDeviceState::Removed }
            }
            "active" => OtherDevice::Active { name },
            other => {
                return Err(DbError::Message(format!(
                    "coven_devices lists the device of release file {} as {other}",
                    self.file
                )))
            }
        })
    }
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
