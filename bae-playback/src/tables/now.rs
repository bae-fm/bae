//! The device's `playback_now` row: what playback is doing, written as each
//! of its values changes, and read by the screens' live query with the
//! playing track's display and the queue's repeat mode. Its table is `now.sql`.
//! The fragments here are what other reads take from the row.

use super::preview_target::preview_target_values;
use super::*;
use crate::renderer::picker::kind_column;
use crate::renderer::{CastReceiver, RendererKind};
use crate::{AudioOutputProblem, PlaybackDiagnostic, PlaybackErrorReason};
use crate::{
    LoadingTrack, NowPlayingDisplay, NowPlayingTrack, NowPlayingValues, PlaybackChange,
    PlaybackPauseReason, PlaybackSideCountdown, PlaybackSidePausePrompt, PlaybackState,
    PlaybackValues, PlayingTrack, PreviewState, RemotePlayback,
};
use bae_store::WriteStatements;
use bae_track_audio::PreviewTarget;
use bae_types::mediums::SideHeading;
use bae_util::timestamp::StoredTimestamp as _;

/// The `playback_now` row, as playback writes it.
#[derive(Clone)]
pub struct PlaybackNowRow {
    store: Store,
}

impl PlaybackNowRow {
    /// The row over `store`.
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

/// Follow what playback is doing as its screens show it. A track the library
/// no longer holds shows as removed: the person deleted it from the library,
/// and playback has not yet moved to another track.
pub fn subscribe_now_playing(
    store: &Store,
) -> coven::LiveQuery<NowPlayingValues, bae_store::DbError> {
    store.subscribe_row(PlayingNowParams {}).process(now_playing_of)
}

/// What playback is doing now, as `subscribe_now_playing` reads it.
#[cfg(any(test, feature = "test-utils"))]
pub async fn now_playing(store: &Store) -> Result<NowPlayingValues, DbError> {
    now_playing_of(store.read_row(PlayingNowParams {}).await?)
}

sqlive_compose::fragment!(
    KeptLevel { volume: f32, is_muted: bool },
    r#"
    SELECT now.volume, now.is_muted
    FROM playback_now AS now
    WHERE now.singleton = 1
"#
);

impl KeptLevel {
    /// The volume and whether it is muted.
    pub(super) fn into_level(self) -> Result<(crate::Volume, bool), DbError> {
        let volume = crate::Volume::new(self.volume)
            .ok_or_else(|| DbError::unreadable("playback_now.volume", self.volume))?;
        Ok((volume, self.is_muted))
    }
}

#[cfg(any(test, feature = "test-utils"))]
sqlive_compose::read!(
    writer PlaybackLevel,
    r#"
    SELECT kept.*
    FROM @crate::tables::now::KeptLevel AS kept
"#
);

/// The volume and mute the row holds, which resuming playback restores.
#[cfg(any(test, feature = "test-utils"))]
pub async fn playback_level(store: &Store) -> Result<(crate::Volume, bool), DbError> {
    store.read_row(PlaybackLevelParams {}).await?.kept.into_level()
}

sqlive_compose::fragment!(
    Casting {
        cast_receiver_id: Option<String>,
        cast_receiver_name: Option<String>,
        cast_receiver_kind: Option<crate::renderer::RendererKind>,
        cast_receiver_address: Option<String>,
        cast_receiver_takes_volume: Option<bool>
    },
    r#"
    SELECT now.remote, now.cast_receiver_id, now.cast_receiver_name, now.cast_receiver_kind,
           now.cast_receiver_model, now.cast_receiver_address, now.cast_receiver_takes_volume
    FROM playback_now AS now
    WHERE now.remote = 'on'
"#
);

impl Casting {
    /// The receiver playback is cast to. The table's CHECKs give a row that
    /// casts every column of its receiver but the model.
    pub(crate) fn into_receiver(self) -> Result<CastReceiver, DbError> {
        StoredReceiver {
            id: self.cast_receiver_id,
            name: self.cast_receiver_name,
            kind: self.cast_receiver_kind,
            model: self.cast_receiver_model,
            address: self.cast_receiver_address,
            takes_volume: self.cast_receiver_takes_volume,
        }
        .read(&CAST_RECEIVER)
    }
}

sqlive_compose::fragment!(
    PreviewedWindow {
        previewed_state: String,
        previewed_path: Option<Vec<u8>>,
        previewed_start_sample: Option<u64>,
        previewed_end_sample: Option<u64>
    },
    r#"
    SELECT now.preview AS previewed_state, now.preview_path AS previewed_path,
           now.preview_start_sample AS previewed_start_sample,
           now.preview_end_sample AS previewed_end_sample
    FROM playback_now AS now
    WHERE now.preview <> 'idle'
"#
);

impl PreviewedWindow {
    /// The stretch of a file the preview player holds, playing or paused.
    /// The table's CHECKs give a row that previews its path and its start.
    pub fn target(self) -> Result<PreviewTarget, DbError> {
        Ok(PreviewTarget::sample_range(
            bae_store::path_column::decode(required(self.previewed_path, "preview_path")?)?,
            required(self.previewed_start_sample, "preview_start_sample")?,
            self.previewed_end_sample,
        ))
    }
}

sqlive_compose::write!(
    SetPlaybackState(
        state: &'static str,
        track_id: Option<&'p str>,
        prepared: Option<bool>,
        waits_on_storage: Option<bool>,
        duration_ms: Option<i64>,
        pause_reason: Option<&'static str>,
        side_boundary_id: Option<&'p str>,
        ended_side_kind: Option<&'static str>,
        ended_side: Option<&'p str>,
        side_resumes_at: Option<i64>,
    ),
    r#"
    UPDATE playback_now
    SET state = :state, track_id = :track_id, prepared = :prepared,
        waits_on_storage = :waits_on_storage, duration_ms = :duration_ms,
        pause_reason = :pause_reason, side_boundary_id = :side_boundary_id,
        ended_side_kind = :ended_side_kind, ended_side = :ended_side,
        side_resumes_at = :side_resumes_at
"#
);

sqlive_compose::write!(
    SetPlaybackVolume(volume: f32),
    r#"UPDATE playback_now SET volume = :volume"#
);

sqlive_compose::write!(
    SetPlaybackMuted(is_muted: bool),
    r#"UPDATE playback_now SET is_muted = :is_muted"#
);

sqlive_compose::write!(
    SetRemotePlayback(
        remote: &'static str,
        receiver_id: Option<&'p str>,
        receiver_name: Option<&'p str>,
        receiver_kind: Option<&'static str>,
        receiver_model: Option<&'p str>,
        receiver_address: Option<&'p str>,
        receiver_takes_volume: Option<bool>,
        end_detail: Option<&'p str>,
    ),
    r#"
    UPDATE playback_now
    SET remote = :remote, cast_receiver_id = :receiver_id, cast_receiver_name = :receiver_name,
        cast_receiver_kind = :receiver_kind, cast_receiver_model = :receiver_model,
        cast_receiver_address = :receiver_address,
        cast_receiver_takes_volume = :receiver_takes_volume, remote_end_detail = :end_detail
"#
);

sqlive_compose::write!(
    SetPlaybackPreview(
        preview: &'static str,
        path: Option<&'p [u8]>,
        start_sample: Option<u64>,
        end_sample: Option<u64>,
        duration_ms: Option<u64>,
    ),
    r#"
    UPDATE playback_now
    SET preview = :preview, preview_path = :path, preview_start_sample = :start_sample,
        preview_end_sample = :end_sample, preview_duration_ms = :duration_ms
"#
);

sqlive_compose::write!(
    SetPlaybackFailure(
        failure: &'static str,
        problem: Option<&'static str>,
        detail: Option<&'p str>,
        device: Option<&'p str>,
        device_state: Option<coven::DeviceState>,
        receiver_id: Option<&'p str>,
        receiver_name: Option<&'p str>,
        receiver_kind: Option<&'static str>,
        receiver_model: Option<&'p str>,
        receiver_address: Option<&'p str>,
        receiver_takes_volume: Option<bool>,
    ),
    r#"
    UPDATE playback_now
    SET failure = :failure, failure_problem = :problem, failure_detail = :detail,
        failure_device = :device, failure_device_state = :device_state,
        failure_receiver_id = :receiver_id, failure_receiver_name = :receiver_name,
        failure_receiver_kind = :receiver_kind, failure_receiver_model = :receiver_model,
        failure_receiver_address = :receiver_address,
        failure_receiver_takes_volume = :receiver_takes_volume
"#
);

// The row's failure cleared, which ends the failure notice.
sqlive_compose::write!(
    ClearPlaybackFailure,
    r#"
    UPDATE playback_now
    SET failure = NULL, failure_problem = NULL, failure_detail = NULL,
        failure_device = NULL, failure_device_state = NULL,
        failure_receiver_id = NULL, failure_receiver_name = NULL,
        failure_receiver_kind = NULL, failure_receiver_model = NULL,
        failure_receiver_address = NULL, failure_receiver_takes_volume = NULL
    WHERE failure IS NOT NULL
"#
);

// Every column but the volume and mute, as playback starts with them.
sqlive_compose::write!(
    ResetPlaybackNow,
    r#"
    UPDATE playback_now
    SET state = 'stopped', track_id = NULL, prepared = NULL, waits_on_storage = NULL,
        duration_ms = NULL, pause_reason = NULL, side_boundary_id = NULL,
        ended_side_kind = NULL, ended_side = NULL, side_resumes_at = NULL,
        remote = 'local', cast_receiver_id = NULL, cast_receiver_name = NULL,
        cast_receiver_kind = NULL, cast_receiver_model = NULL, cast_receiver_address = NULL,
        cast_receiver_takes_volume = NULL, remote_end_detail = NULL, preview = 'idle',
        preview_path = NULL, preview_start_sample = NULL, preview_end_sample = NULL,
        preview_duration_ms = NULL, failure = NULL, failure_problem = NULL,
        failure_detail = NULL, failure_device = NULL, failure_device_state = NULL,
        failure_receiver_id = NULL, failure_receiver_name = NULL,
        failure_receiver_kind = NULL, failure_receiver_model = NULL,
        failure_receiver_address = NULL, failure_receiver_takes_volume = NULL
"#
);

// The volume full and unmuted.
sqlive_compose::write!(ResetPlaybackLevel, r#"UPDATE playback_now SET volume = 1.0, is_muted = 0"#);

impl PlaybackNowRow {
    /// Write `change` into the row. A state that plays clears the last
    /// failure, and a new failure replaces the one the row holds.
    pub async fn apply(&self, change: &PlaybackChange) -> Result<(), DbError> {
        let change = change.clone();
        self.store.call(move |sql| apply_on(sql, &change).map_err(DbError::from)).await
    }
}

fn apply_on(sql: &CovenWrite<'_>, change: &PlaybackChange) -> rusqlite::Result<()> {
    match change {
        PlaybackChange::State(state) => {
            let columns = StateColumns::of(state);
            sql.run(&SetPlaybackStateParams {
                state: columns.state,
                track_id: columns.track_id,
                prepared: columns.prepared,
                waits_on_storage: columns.waits_on_storage,
                duration_ms: columns.duration_ms,
                pause_reason: columns.pause_reason,
                side_boundary_id: columns.side_boundary_id,
                ended_side_kind: columns.ended_side_kind,
                ended_side: columns.ended_side.as_deref(),
                side_resumes_at: columns.side_resumes_at,
            })?;
            if matches!(state, PlaybackState::Playing { .. }) {
                sql.run(&ClearPlaybackFailureParams {})?;
            }
        }
        PlaybackChange::Volume(volume) => {
            sql.run(&SetPlaybackVolumeParams { volume: *volume })?;
        }
        PlaybackChange::Muted(muted) => {
            sql.run(&SetPlaybackMutedParams { is_muted: *muted })?;
        }
        PlaybackChange::Remote(remote) => {
            let (kind, device, detail) = match remote {
                RemotePlayback::Local => ("local", None, None),
                RemotePlayback::On(device) => ("on", Some(device), None),
                RemotePlayback::Ended { device, detail } => {
                    ("ended", Some(device), Some(detail.as_str()))
                }
            };
            let device = ReceiverValues::of(device);
            sql.run(&SetRemotePlaybackParams {
                remote: kind,
                receiver_id: device.id,
                receiver_name: device.name,
                receiver_kind: device.kind,
                receiver_model: device.model,
                receiver_address: device.address,
                receiver_takes_volume: device.takes_volume,
                end_detail: detail,
            })?;
        }
        PlaybackChange::Preview(preview) => {
            let (kind, clip) = match preview {
                PreviewState::Idle => ("idle", None),
                PreviewState::Playing { target, duration_ms } => {
                    ("playing", Some((target, duration_ms)))
                }
                PreviewState::Paused { target, duration_ms } => {
                    ("paused", Some((target, duration_ms)))
                }
            };
            let clip = clip.map(|(target, duration_ms)| {
                let (path, start_sample, end_sample) = preview_target_values(target);
                (path, start_sample, end_sample, *duration_ms)
            });
            sql.run(&SetPlaybackPreviewParams {
                preview: kind,
                path: clip.as_ref().map(|clip| clip.0.as_slice()),
                start_sample: clip.as_ref().map(|clip| clip.1),
                end_sample: clip.as_ref().and_then(|clip| clip.2),
                duration_ms: clip.as_ref().map(|clip| clip.3),
            })?;
        }
        PlaybackChange::Failed(reason) => {
            let columns = FailureColumns::of(reason);
            let receiver = ReceiverValues::of(columns.receiver);
            sql.run(&SetPlaybackFailureParams {
                failure: columns.failure,
                problem: columns.problem,
                detail: columns.detail,
                device: columns.device,
                device_state: columns.device_state,
                receiver_id: receiver.id,
                receiver_name: receiver.name,
                receiver_kind: receiver.kind,
                receiver_model: receiver.model,
                receiver_address: receiver.address,
                receiver_takes_volume: receiver.takes_volume,
            })?;
        }
    }
    Ok(())
}

/// The person dismissed the failure notice: clear the failure the row holds,
/// so the notice shows again only for a later failure.
pub async fn dismiss_playback_failure(store: &Store) -> Result<(), DbError> {
    store
        .call(|sql| {
            sql.run(&ClearPlaybackFailureParams {})?;
            Ok(())
        })
        .await
}

/// Reset the row to what playback starts with: stopped, on this device's
/// output, previewing nothing, no failure. The volume and mute are kept when
/// `keep_level`, for playback to resume at; otherwise full and unmuted.
pub(super) fn reset_playback_now_on(
    sql: &coven::BulkSqlContext<'_>,
    keep_level: bool,
) -> rusqlite::Result<()> {
    sql.run(&ResetPlaybackNowParams {})?;
    if !keep_level {
        sql.run(&ResetPlaybackLevelParams {})?;
    }
    Ok(())
}

/// A state as the row's state columns hold it.
struct StateColumns<'a> {
    state: &'static str,
    track_id: Option<&'a str>,
    prepared: Option<bool>,
    waits_on_storage: Option<bool>,
    duration_ms: Option<i64>,
    pause_reason: Option<&'static str>,
    side_boundary_id: Option<&'a str>,
    ended_side_kind: Option<&'static str>,
    ended_side: Option<String>,
    side_resumes_at: Option<i64>,
}

impl<'a> StateColumns<'a> {
    fn of(state: &'a PlaybackState) -> Self {
        let mut columns = Self {
            state: "stopped",
            track_id: None,
            prepared: None,
            waits_on_storage: None,
            duration_ms: None,
            pause_reason: None,
            side_boundary_id: None,
            ended_side_kind: None,
            ended_side: None,
            side_resumes_at: None,
        };
        let prepared_track = |columns: &mut Self, track: &'a PlayingTrack| {
            columns.track_id = Some(&track.track_id);
            columns.duration_ms = track.duration_ms.map(|duration| duration as i64);
        };
        match state {
            PlaybackState::Stopped => {}
            PlaybackState::Loading { track, waits_on_storage } => {
                columns.state = "loading";
                columns.waits_on_storage = Some(*waits_on_storage);
                match track {
                    LoadingTrack::Unprepared(track_id) => {
                        columns.track_id = Some(track_id);
                        columns.prepared = Some(false);
                    }
                    LoadingTrack::Prepared(track) => {
                        prepared_track(&mut columns, track);
                        columns.prepared = Some(true);
                    }
                }
            }
            PlaybackState::Playing { track } => {
                columns.state = "playing";
                prepared_track(&mut columns, track);
            }
            PlaybackState::Paused { track, reason } => {
                columns.state = "paused";
                prepared_track(&mut columns, track);
                match reason {
                    PlaybackPauseReason::Manual => columns.pause_reason = Some("manual"),
                    PlaybackPauseReason::SideEnded(prompt) => {
                        columns.pause_reason = Some("side_ended");
                        columns.side_boundary_id = Some(&prompt.id);
                        let (kind, ended) = match &prompt.ended {
                            SideHeading::Sided { side_letter } => ("sided", side_letter.clone()),
                            SideHeading::Disc { disc } => ("disc", disc.to_string()),
                        };
                        columns.ended_side_kind = Some(kind);
                        columns.ended_side = Some(ended);
                        columns.side_resumes_at = prompt
                            .countdown
                            .as_ref()
                            .map(|countdown| countdown.resumes_at.unix_timestamp_millis());
                    }
                }
            }
        }
        columns
    }
}

/// A failure as the row's failure columns hold it.
struct FailureColumns<'a> {
    failure: &'static str,
    problem: Option<&'static str>,
    detail: Option<&'a str>,
    device: Option<&'a str>,
    device_state: Option<coven::DeviceState>,
    receiver: Option<&'a CastReceiver>,
}

/// A receiver's values in the order a row's receiver columns hold them (id,
/// name, kind, model, address, whether it takes a volume), all NULL for none.
struct ReceiverValues<'a> {
    id: Option<&'a str>,
    name: Option<&'a str>,
    kind: Option<&'static str>,
    model: Option<&'a str>,
    address: Option<&'a str>,
    takes_volume: Option<bool>,
}

impl<'a> ReceiverValues<'a> {
    fn of(receiver: Option<&'a CastReceiver>) -> Self {
        ReceiverValues {
            id: receiver.map(|receiver| receiver.id.as_str()),
            name: receiver.map(|receiver| receiver.name.as_str()),
            kind: receiver.map(|receiver| kind_column(receiver.kind)),
            model: receiver.and_then(|receiver| receiver.model.as_deref()),
            address: receiver.map(|receiver| receiver.address.as_str()),
            takes_volume: receiver.map(|receiver| receiver.takes_volume),
        }
    }
}

/// The columns a row names a receiver in: the one playback is on, or the one
/// a failure names.
struct ReceiverColumns {
    id: &'static str,
    name: &'static str,
    kind: &'static str,
    address: &'static str,
    takes_volume: &'static str,
}

/// The receiver playback is on, or lost the link to.
const CAST_RECEIVER: ReceiverColumns = ReceiverColumns {
    id: "cast_receiver_id",
    name: "cast_receiver_name",
    kind: "cast_receiver_kind",
    address: "cast_receiver_address",
    takes_volume: "cast_receiver_takes_volume",
};

/// The receiver that no longer accepts this device.
const FAILURE_RECEIVER: ReceiverColumns = ReceiverColumns {
    id: "failure_receiver_id",
    name: "failure_receiver_name",
    kind: "failure_receiver_kind",
    address: "failure_receiver_address",
    takes_volume: "failure_receiver_takes_volume",
};

impl<'a> FailureColumns<'a> {
    fn of(reason: &'a PlaybackErrorReason) -> Self {
        let columns = |failure| Self {
            failure,
            problem: None,
            detail: None,
            device: None,
            device_state: None,
            receiver: None,
        };
        match reason {
            PlaybackErrorReason::SyncDisconnected => columns("sync_disconnected"),
            PlaybackErrorReason::OnOtherDevice(device) => {
                let name = match device {
                    bae_records::OtherDevice::Active { name }
                    | bae_records::OtherDevice::Former { name, .. } => name,
                };
                Self {
                    device: Some(name),
                    device_state: Some(device.state()),
                    ..columns("on_other_device")
                }
            }
            PlaybackErrorReason::OriginalMissing => columns("original_missing"),
            PlaybackErrorReason::AudioOutput { problem } => Self {
                problem: Some(match problem {
                    AudioOutputProblem::DeviceUnavailable => "device_unavailable",
                    AudioOutputProblem::UnsupportedFormat => "unsupported_format",
                    AudioOutputProblem::AirPlayRefused => "airplay_refused",
                    AudioOutputProblem::AirPlayPairingLost { .. } => "airplay_pairing_lost",
                }),
                receiver: match problem {
                    AudioOutputProblem::AirPlayPairingLost { receiver } => Some(receiver),
                    AudioOutputProblem::DeviceUnavailable
                    | AudioOutputProblem::UnsupportedFormat
                    | AudioOutputProblem::AirPlayRefused => None,
                },
                ..columns("audio_output")
            },
            PlaybackErrorReason::Diagnostic { failure, detail } => Self {
                detail: Some(detail),
                ..columns(match failure {
                    PlaybackDiagnostic::Internal => "internal",
                    PlaybackDiagnostic::Renderer => "renderer",
                })
            },
        }
    }
}

/// A column value the schema's CHECKs rule out: the row is one nothing here
/// wrote.
fn unknown(column: &str, value: &str) -> DbError {
    DbError::unreadable(format!("playback_now.{column}"), value)
}

/// `value`, which the schema's CHECKs give the row in `column`.
fn required<T>(value: Option<T>, column: &str) -> Result<T, DbError> {
    value.ok_or_else(|| DbError::unreadable(format!("playback_now.{column}"), None::<String>))
}

/// A receiver as the columns that name one hold it.
struct StoredReceiver {
    id: Option<String>,
    name: Option<String>,
    kind: Option<RendererKind>,
    model: Option<String>,
    address: Option<String>,
    takes_volume: Option<bool>,
}

impl StoredReceiver {
    /// The receiver these values name, from the row's `columns`; every one
    /// but the model is present.
    fn read(self, columns: &ReceiverColumns) -> Result<CastReceiver, DbError> {
        Ok(CastReceiver {
            id: required(self.id, columns.id)?,
            name: required(self.name, columns.name)?,
            kind: required(self.kind, columns.kind)?,
            model: self.model,
            address: required(self.address, columns.address)?,
            takes_volume: required(self.takes_volume, columns.takes_volume)?,
        })
    }
}

// The row as the screens show it, with the queue's repeat mode, the stretch
// of a file it previews, and the current display of the track it names,
// loading, playing or paused; the display is NULL when the row names no track
// the library holds.
sqlive_compose::read!(
    writer PlayingNow {
        prepared: Option<bool>,
        waits_on_storage: Option<bool>,
        duration_ms: Option<u64>,
        side_resumes_at: Option<i64>,
        volume: f32,
        is_muted: bool,
        cast_receiver_kind: Option<crate::renderer::RendererKind>,
        cast_receiver_takes_volume: Option<bool>,
        preview_duration_ms: Option<u64>,
        failure_device_state: Option<coven::DeviceState>,
        failure_receiver_kind: Option<crate::renderer::RendererKind>,
        failure_receiver_takes_volume: Option<bool>
    },
    r#"
    SELECT now.state, now.track_id, now.prepared, now.waits_on_storage, now.duration_ms,
           now.pause_reason, now.side_boundary_id, now.ended_side_kind, now.ended_side,
           now.side_resumes_at, now.volume, now.is_muted,
           now.remote, now.cast_receiver_id, now.cast_receiver_name, now.cast_receiver_kind,
           now.cast_receiver_model, now.cast_receiver_address, now.cast_receiver_takes_volume,
           now.remote_end_detail, now.preview_duration_ms,
           now.failure, now.failure_problem, now.failure_detail, now.failure_device,
           now.failure_device_state, now.failure_receiver_id, now.failure_receiver_name,
           now.failure_receiver_kind, now.failure_receiver_model, now.failure_receiver_address,
           now.failure_receiver_takes_volume,
           queued.*, previewed.*, shown.*
    FROM playback_now AS now
    JOIN @crate::tables::queue_reads::QueueRepeat AS queued ON 1
    LEFT JOIN @crate::tables::now::PreviewedWindow AS previewed ON 1
    LEFT JOIN @bae_records::records::track_display::DisplayedTrack AS shown
           ON shown.display_track_id = now.track_id
"#
);

impl PlayingNowRow {
    /// The row's track state, apart from the track's display.
    fn state(&self) -> Result<PlaybackState, DbError> {
        if self.state == "stopped" {
            return Ok(PlaybackState::Stopped);
        }
        let track_id = required(self.track_id.clone(), "track_id")?;
        let track = PlayingTrack { track_id, duration_ms: self.duration_ms };
        Ok(match self.state.as_str() {
            "loading" => PlaybackState::Loading {
                track: if required(self.prepared, "prepared")? {
                    LoadingTrack::Prepared(track)
                } else {
                    LoadingTrack::Unprepared(track.track_id)
                },
                waits_on_storage: required(self.waits_on_storage, "waits_on_storage")?,
            },
            "playing" => PlaybackState::Playing { track },
            "paused" => PlaybackState::Paused { track, reason: self.pause_reason()? },
            other => return Err(unknown("state", other)),
        })
    }

    /// Why the paused row is paused.
    fn pause_reason(&self) -> Result<PlaybackPauseReason, DbError> {
        let reason = required(self.pause_reason.as_deref(), "pause_reason")?;
        Ok(match reason {
            "manual" => PlaybackPauseReason::Manual,
            "side_ended" => {
                let kind = required(self.ended_side_kind.as_deref(), "ended_side_kind")?;
                let ended = required(self.ended_side.clone(), "ended_side")?;
                PlaybackPauseReason::SideEnded(PlaybackSidePausePrompt {
                    id: required(self.side_boundary_id.clone(), "side_boundary_id")?,
                    ended: match kind {
                        "sided" => SideHeading::Sided { side_letter: ended },
                        "disc" => SideHeading::Disc {
                            disc: ended
                                .parse()
                                .map_err(|_not_a_number| unknown("ended_side", &ended))?,
                        },
                        other => return Err(unknown("ended_side_kind", other)),
                    },
                    countdown: self
                        .side_resumes_at
                        .map(|millis| {
                            bae_util::timestamp::from_unix_timestamp_millis(millis)
                                .map(|resumes_at| PlaybackSideCountdown { resumes_at })
                                .map_err(|_out_of_range| {
                                    unknown("side_resumes_at", &millis.to_string())
                                })
                        })
                        .transpose()?,
                })
            }
            other => return Err(unknown("pause_reason", other)),
        })
    }

    /// Where playback goes.
    fn remote(&self) -> Result<RemotePlayback, DbError> {
        if self.remote == "local" {
            return Ok(RemotePlayback::Local);
        }
        let device = StoredReceiver {
            id: self.cast_receiver_id.clone(),
            name: self.cast_receiver_name.clone(),
            kind: self.cast_receiver_kind,
            model: self.cast_receiver_model.clone(),
            address: self.cast_receiver_address.clone(),
            takes_volume: self.cast_receiver_takes_volume,
        }
        .read(&CAST_RECEIVER)?;
        Ok(match self.remote.as_str() {
            "on" => RemotePlayback::On(device),
            "ended" => RemotePlayback::Ended {
                device,
                detail: required(self.remote_end_detail.clone(), "remote_end_detail")?,
            },
            other => return Err(unknown("remote", other)),
        })
    }

    /// What the preview player holds.
    fn preview(&self) -> Result<PreviewState, DbError> {
        let Some(previewed) = &self.previewed else {
            return Ok(PreviewState::Idle);
        };
        let target = previewed.clone().target()?;
        let duration_ms = required(self.preview_duration_ms, "preview_duration_ms")?;
        Ok(match previewed.previewed_state.as_str() {
            "playing" => PreviewState::Playing { target, duration_ms },
            "paused" => PreviewState::Paused { target, duration_ms },
            other => return Err(unknown("preview", other)),
        })
    }

    /// Why playback last failed.
    fn failure(&self) -> Result<Option<PlaybackErrorReason>, DbError> {
        let Some(failure) = self.failure.as_deref() else {
            return Ok(None);
        };
        let diagnostic = |failure| -> Result<PlaybackErrorReason, DbError> {
            let detail = required(self.failure_detail.clone(), "failure_detail")?;
            Ok(PlaybackErrorReason::Diagnostic { failure, detail })
        };
        let reason = match failure {
            "sync_disconnected" => PlaybackErrorReason::SyncDisconnected,
            "on_other_device" => PlaybackErrorReason::OnOtherDevice(bae_records::OtherDevice::of(
                required(self.failure_device.clone(), "failure_device")?,
                required(self.failure_device_state, "failure_device_state")?,
            )),
            "original_missing" => PlaybackErrorReason::OriginalMissing,
            "audio_output" => {
                let problem = required(self.failure_problem.as_deref(), "failure_problem")?;
                PlaybackErrorReason::AudioOutput {
                    problem: match problem {
                        "device_unavailable" => AudioOutputProblem::DeviceUnavailable,
                        "unsupported_format" => AudioOutputProblem::UnsupportedFormat,
                        "airplay_refused" => AudioOutputProblem::AirPlayRefused,
                        "airplay_pairing_lost" => AudioOutputProblem::AirPlayPairingLost {
                            receiver: StoredReceiver {
                                id: self.failure_receiver_id.clone(),
                                name: self.failure_receiver_name.clone(),
                                kind: self.failure_receiver_kind,
                                model: self.failure_receiver_model.clone(),
                                address: self.failure_receiver_address.clone(),
                                takes_volume: self.failure_receiver_takes_volume,
                            }
                            .read(&FAILURE_RECEIVER)?,
                        },
                        other => return Err(unknown("failure_problem", other)),
                    },
                }
            }
            "internal" => diagnostic(PlaybackDiagnostic::Internal)?,
            "renderer" => diagnostic(PlaybackDiagnostic::Renderer)?,
            other => return Err(unknown("failure", other)),
        };
        Ok(Some(reason))
    }
}

/// The row as the screens show it, with the queue's repeat mode and the
/// current display of the track it names; a track the library no longer holds
/// shows as removed.
/// What playback is doing now, as its row holds it.
fn now_playing_of(row: PlayingNowRow) -> Result<NowPlayingValues, DbError> {
    let values = PlaybackValues {
        state: row.state()?,
        volume: row.volume,
        is_muted: row.is_muted,
        remote: row.remote()?,
        preview: row.preview()?,
    };
    let failure = row.failure()?;
    let display = row.shown.map(|shown| shown.into_display()).transpose()?;
    let display = NowPlayingDisplay::of(display);
    let playback = values.map_tracks(
        |track| NowPlayingTrack { track, display: display.clone() },
        |track_id| NowPlayingTrack { track: track_id, display: display.clone() },
    );
    Ok(NowPlayingValues { playback, repeat_mode: row.queued.into_mode(), failure })
}

/// Plant damage for a test: the table `playback_now` is gone, so the volume
/// and mute it kept can't be read.
#[cfg(any(test, feature = "test-utils"))]
pub fn lose_playback_level_for_test(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "playback_now",
        "DROP TABLE playback_now;",
    );
}

#[cfg(test)]
#[path = "now_tests.rs"]
mod tests;
