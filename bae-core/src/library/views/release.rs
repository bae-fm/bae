//! The release detail view: one release with its album's credits, its place
//! among the album's releases, and the state it is resolved against.

use super::*;
use crate::album_detail::{ReleaseDetail, ReleaseResolveCtx};

impl LibraryViews {
    /// Resolved release detail for the album-detail view: a `ReleaseSummary` plus
    /// the tracks/files/gallery its SQL joins load, and the release's position
    /// within its album, so `display_name` needs no index from the caller. `None`
    /// when the release doesn't exist.
    pub async fn find_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseDetail>, DbError> {
        let Some(context) = self.find_release_detail_context(release_id).await? else {
            return Ok(None);
        };
        let has_cloud_home =
            !matches!(self.database.coven_sync_status().await?, coven::SyncStatus::Disconnected);
        Ok(Some(resolve_release_detail(release_id, context, has_cloud_home)))
    }

    /// The release's rows as its detail reads them, unresolved; `None` when
    /// the release doesn't exist.
    pub async fn find_db_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<DbReleaseDetail>, DbError> {
        let release_id = release_id.to_string();
        self.database
            .read(move |sql| {
                let Some(release) = find_release_by_id_on(&sql, &release_id)? else {
                    return Ok(None);
                };
                Ok(Some(build_release_detail_on(&sql, release)?))
            })
            .process(|rows| Ok(rows.map(ReleaseDetailRows::process)))
            .await
    }

    /// The raw release-detail aggregate, the release's album artists, and its
    /// 0-based position among its album's releases — all from one database call.
    pub(crate) async fn find_release_detail_context(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseDetailContext>, DbError> {
        let release_id = release_id.to_string();
        self.database
            .read(move |sql| find_release_detail_context_on(&sql, &release_id))
            .process(|rows| Ok(rows.map(ReleaseContextRows::process)))
            .await
    }

    /// Follow the release `initial` names, or none. The detail view moves
    /// to another release through the request handle, not another query.
    pub(crate) fn subscribe_release_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ReleaseDetailProjection> {
        self.database
            .subscribe_reconfigurable(initial, |release_id, sql| {
                let has_cloud_home = cloud_home_on(&sql)?;
                let context = match release_id {
                    Some(release_id) => find_release_detail_context_on(&sql, release_id)
                        .map_err(CovenError::from)?,
                    None => None,
                };
                Ok((context, has_cloud_home))
            })
            .process(|(context, has_cloud_home)| {
                Ok(ReleaseDetailProjection {
                    context: context.map(ReleaseContextRows::process),
                    has_cloud_home,
                })
            })
    }
}

/// One delivery of the release `release_id` as the detail view shows it.
pub(crate) fn resolve_release_detail_projection(
    release_id: &str,
    projection: ReleaseDetailProjection,
) -> Option<ReleaseDetail> {
    let has_cloud_home = projection.has_cloud_home;
    projection.context.map(|context| resolve_release_detail(release_id, context, has_cloud_home))
}

/// The release's detail as its view shows it, against whether storage is
/// set up.
fn resolve_release_detail(
    release_id: &str,
    context: ReleaseDetailContext,
    has_cloud_home: bool,
) -> ReleaseDetail {
    let ReleaseDetailContext { detail, album_artists, release_index, is_compilation } = context;
    let ctx = ReleaseResolveCtx {
        has_cloud_home,
        cover: crate::db::cover_ref(release_id, detail.cover_version.clone()),
        is_compilation,
    };
    ReleaseDetail::from_raw(detail, &album_artists, release_index, &ctx)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseDetailProjection {
    pub context: Option<ReleaseDetailContext>,
    /// Whether storage is set up on this device, as coven's sync status
    /// says.
    pub has_cloud_home: bool,
}

/// How many releases of its album come before the release `?1` in the
/// album's order (`album_order`, then id): its index among them, counted on
/// the album's releases index.
pub(crate) const RELEASE_INDEX_IN_ALBUM: &str = "SELECT COUNT(*) FROM releases AS shown \
     JOIN releases AS earlier ON earlier.album_id = shown.album_id \
      AND (earlier.album_order < shown.album_order \
           OR (earlier.album_order = shown.album_order AND earlier.id < shown.id)) \
     WHERE shown.id = ?1";

/// The release's detail rows with its album's credits, its index among the
/// album's releases, and whether the album is a compilation; `None` when
/// the release doesn't exist.
pub(crate) fn find_release_detail_context_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseContextRows>, DbError> {
    let Some(release) = find_release_by_id_on(sql, release_id)? else {
        return Ok(None);
    };
    let album_artists = get_artists_for_album_on(sql, &release.album_id)?;
    let release_index: i64 =
        sql.query_row(RELEASE_INDEX_IN_ALBUM, params![release.id], |row| row.get(0))?;
    let release_index = usize::try_from(release_index)
        .map_err(|error| DbError::Message(format!("release index {release_index}: {error}")))?;
    let is_compilation =
        find_album_by_id_on(sql, &release.album_id)?.is_some_and(|album| album.is_compilation);
    Ok(Some(ReleaseContextRows {
        detail: build_release_detail_on(sql, release)?,
        album_artists,
        release_index,
        is_compilation,
    }))
}

#[derive(Clone, PartialEq)]
pub(crate) struct ReleaseContextRows {
    detail: ReleaseDetailRows,
    album_artists: Vec<LibraryCredit>,
    release_index: usize,
    is_compilation: bool,
}

impl ReleaseContextRows {
    pub(crate) fn process(self) -> ReleaseDetailContext {
        ReleaseDetailContext {
            detail: self.detail.process(),
            album_artists: self.album_artists,
            release_index: self.release_index,
            is_compilation: self.is_compilation,
        }
    }
}

/// The release's rows as its detail reads them: its tracks with their
/// credits, its files, records, mediums and audio on one device only, and
/// whether it is pinned, stored, queued to upload or download, or moving.
pub(crate) fn build_release_detail_on(
    sql: &SqlReadContext<'_>,
    release: DbRelease,
) -> Result<ReleaseDetailRows, DbError> {
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
    let download = crate::db::client::release_queues::download_phase_on(sql, &release.id)?;
    let cover_version = sql.query_row(
        &format!("SELECT {}", cover_version_sql("?1")),
        params![release.id],
        |row| row.get(0),
    )?;
    let upload = crate::db::client::queued_uploads::release_upload_on(sql, &release.id)?;
    let transfer_action = sql.query_row(
        &format!("SELECT {} AS transfer_action", transfer_action_sql("?1")),
        params![release.id],
        |row| transfer_action_column(row, "transfer_action"),
    )?;

    Ok(ReleaseDetailRows {
        release,
        mediums,
        tracks,
        files,
        records,
        unstored_audio,
        pinned,
        remote,
        upload,
        download,
        transfer_action,
        cover_version,
    })
}

#[derive(Clone, PartialEq)]
pub(crate) struct ReleaseDetailRows {
    pub(crate) release: DbRelease,
    mediums: DbReleaseMediums,
    tracks: Vec<(DbTrack, Option<LibraryCredit>)>,
    files: Vec<DbFile>,
    records: Vec<crate::catalog_model::records::ReleaseRecord>,
    unstored_audio: Vec<DbUnstoredAudio>,
    pinned: bool,
    remote: bool,
    upload: Option<DbReleaseUpload>,
    download: Option<crate::library::DownloadPhase>,
    transfer_action: Option<bae_types::release_storage::ReleaseStorageAction>,
    cover_version: Option<String>,
}

impl ReleaseDetailRows {
    pub(crate) fn process(self) -> DbReleaseDetail {
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
            download: self.download,
            transfer_action: self.transfer_action,
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
pub(crate) fn unstored_audio_on(
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
    fn other_device(&self) -> Result<crate::album_detail::OtherDevice, DbError> {
        use crate::album_detail::OtherDevice;
        use crate::sync::membership::FormerDeviceState;
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

/// [`unstored_audio_on`] for a row mapper reading a release's summary, its
/// failure carried as the mapper's error.
pub(crate) fn unstored_audio_in_row(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> rusqlite::Result<Vec<DbUnstoredAudio>> {
    unstored_audio_on(sql, release_id)
        .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))
}
