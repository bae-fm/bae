//! The device's `playback_now` row: what playback is doing, written as each
//! of its values changes, and read by the screens' live query with the
//! playing track's display and the queue's repeat mode. Its table is `now.sql`.

/// The row's part of the library's schema.
pub(crate) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("now.sql")]);

use super::preview_target::{preview_target_from_row, preview_target_values};
use super::queue::repeat_from_row;
use super::track_display::track_display_on;
use super::*;
use crate::renderer::picker::{kind_column, kind_from_column};
use crate::renderer::CastReceiver;
use crate::{AudioOutputProblem, PlaybackDiagnostic, PlaybackErrorReason};
use crate::{
    LoadingTrack, NowPlayingTrack, NowPlayingValues, PlaybackChange, PlaybackFailure,
    PlaybackPauseBoundary, PlaybackPauseReason, PlaybackSideCountdown, PlaybackSidePausePrompt,
    PlaybackState, PlaybackValues, PlayingTrack, PreviewState, RemotePlayback,
};

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

/// Follow what playback is doing as its screens show it. `None` while the
/// row names a track the library no longer holds: the track was deleted, and
/// playback has yet to move off it.
pub fn subscribe_now_playing(
    store: &Store,
) -> coven::LiveQuery<Option<NowPlayingValues>, bae_store::DbError> {
    store.subscribe(|sql| now_playing_on(&sql))
}

/// What playback is doing now, as `subscribe_now_playing` reads it.
#[cfg(any(test, feature = "test-utils"))]
pub async fn now_playing(store: &Store) -> Result<Option<NowPlayingValues>, DbError> {
    store.read(|sql| now_playing_on(&sql)).await
}

/// The volume and mute the row holds, which resuming playback restores.
pub async fn playback_level(store: &Store) -> Result<(crate::Volume, bool), DbError> {
    store
        .read(|sql| {
            Ok(sql.query_row("SELECT volume, is_muted FROM playback_now", [], |row| {
                let stored: f32 = row.get("volume")?;
                let volume = crate::Volume::new(stored).ok_or_else(|| {
                    bae_store::read::column_conversion_error(
                        row,
                        "volume",
                        format!("volume {stored} is no level"),
                    )
                })?;
                Ok((volume, row.get("is_muted")?))
            })?)
        })
        .await
}

impl PlaybackNowRow {
    /// Write `change` into the row. A state that plays clears the last
    /// failure; a failure counts up the row's failures.
    pub async fn apply(&self, change: &PlaybackChange) -> Result<(), DbError> {
        let change = change.clone();
        self.store.call(move |sql| apply_on(sql, &change).map_err(DbError::from)).await
    }
}

fn apply_on(sql: &CovenWrite<'_>, change: &PlaybackChange) -> rusqlite::Result<()> {
    match change {
        PlaybackChange::State(state) => {
            let columns = StateColumns::of(state);
            sql.execute(
                "UPDATE playback_now SET state = ?, track_id = ?, prepared = ?, duration_ms = ?, \
                     pause_reason = ?, side_boundary_id = ?, side_boundary_kind = ?, \
                     side_label = ?, side_resumes_at = ?",
                params![
                    columns.state,
                    columns.track_id,
                    columns.prepared,
                    columns.duration_ms,
                    columns.pause_reason,
                    columns.side_boundary_id,
                    columns.side_boundary_kind,
                    columns.side_label,
                    columns.side_resumes_at,
                ],
            )?;
            if matches!(state, PlaybackState::Playing { .. }) {
                sql.execute(
                    "UPDATE playback_now \
                     SET failure = NULL, failure_problem = NULL, failure_detail = NULL, \
                         failure_device = NULL, failure_device_state = NULL \
                     WHERE failure IS NOT NULL",
                    [],
                )?;
            }
        }
        PlaybackChange::Volume(volume) => {
            sql.execute("UPDATE playback_now SET volume = ?", [volume])?;
        }
        PlaybackChange::Muted(muted) => {
            sql.execute("UPDATE playback_now SET is_muted = ?", [muted])?;
        }
        PlaybackChange::Remote(remote) => {
            let (kind, device, detail) = match remote {
                RemotePlayback::Local => ("local", None, None),
                RemotePlayback::On(device) => ("on", Some(device), None),
                RemotePlayback::Ended { device, detail } => {
                    ("ended", Some(device), Some(detail.as_str()))
                }
            };
            sql.execute(
                "UPDATE playback_now SET remote = ?, cast_receiver_id = ?, \
                     cast_receiver_name = ?, cast_receiver_kind = ?, \
                     cast_receiver_model = ?, cast_receiver_address = ?, \
                     cast_receiver_takes_volume = ?, remote_end_detail = ?",
                params![
                    kind,
                    device.map(|device| device.id.as_str()),
                    device.map(|device| device.name.as_str()),
                    device.map(|device| kind_column(device.kind)),
                    device.and_then(|device| device.model.as_deref()),
                    device.map(|device| device.address.as_str()),
                    device.map(|device| device.takes_volume),
                    detail,
                ],
            )?;
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
            sql.execute(
                "UPDATE playback_now SET preview = ?, preview_path = ?, \
                     preview_start_sample = ?, preview_end_sample = ?, preview_duration_ms = ?",
                params![
                    kind,
                    clip.as_ref().map(|clip| clip.0.as_slice()),
                    clip.as_ref().map(|clip| clip.1),
                    clip.as_ref().and_then(|clip| clip.2),
                    clip.as_ref().map(|clip| clip.3),
                ],
            )?;
        }
        PlaybackChange::Failed(reason) => {
            let columns = FailureColumns::of(reason);
            sql.execute(
                "UPDATE playback_now SET failure = ?, failure_problem = ?, failure_detail = ?, \
                     failure_device = ?, failure_device_state = ?, failures = failures + 1",
                params![
                    columns.failure,
                    columns.problem,
                    columns.detail,
                    columns.device,
                    columns.device_state
                ],
            )?;
        }
    }
    Ok(())
}

/// Reset the row to what playback starts with: stopped, on this device's
/// output, previewing nothing, no failure. The volume and mute are kept when
/// `keep_level`, for playback to resume at; otherwise full and unmuted.
pub(super) fn reset_playback_now_on(
    sql: &coven::BulkSqlContext<'_>,
    keep_level: bool,
) -> rusqlite::Result<()> {
    sql.execute(
        "UPDATE playback_now SET state = 'stopped', track_id = NULL, prepared = NULL, \
             duration_ms = NULL, pause_reason = NULL, side_boundary_id = NULL, \
             side_boundary_kind = NULL, side_label = NULL, side_resumes_at = NULL, \
             remote = 'local', cast_receiver_id = NULL, cast_receiver_name = NULL, \
             cast_receiver_kind = NULL, cast_receiver_model = NULL, \
             cast_receiver_address = NULL, cast_receiver_takes_volume = NULL, \
             remote_end_detail = NULL, preview = 'idle', preview_path = NULL, \
             preview_start_sample = NULL, preview_end_sample = NULL, preview_duration_ms = NULL, \
             failure = NULL, failure_problem = NULL, failure_detail = NULL, \
             failure_device = NULL, failure_device_state = NULL, failures = 0",
        [],
    )?;
    if !keep_level {
        sql.execute("UPDATE playback_now SET volume = 1.0, is_muted = 0", [])?;
    }
    Ok(())
}

/// A state as the row's state columns hold it.
struct StateColumns<'a> {
    state: &'static str,
    track_id: Option<&'a str>,
    prepared: Option<bool>,
    duration_ms: Option<i64>,
    pause_reason: Option<&'static str>,
    side_boundary_id: Option<&'a str>,
    side_boundary_kind: Option<&'static str>,
    side_label: Option<&'a str>,
    side_resumes_at: Option<i64>,
}

impl<'a> StateColumns<'a> {
    fn of(state: &'a PlaybackState) -> Self {
        let mut columns = Self {
            state: "stopped",
            track_id: None,
            prepared: None,
            duration_ms: None,
            pause_reason: None,
            side_boundary_id: None,
            side_boundary_kind: None,
            side_label: None,
            side_resumes_at: None,
        };
        let prepared_track = |columns: &mut Self, track: &'a PlayingTrack| {
            columns.track_id = Some(&track.track_id);
            columns.duration_ms = track.duration_ms.map(|duration| duration as i64);
        };
        match state {
            PlaybackState::Stopped => {}
            PlaybackState::Loading { track } => {
                columns.state = "loading";
                match track {
                    LoadingTrack::Unprepared { track_id } => {
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
                        columns.side_boundary_kind = Some(match prompt.boundary {
                            PlaybackPauseBoundary::Side => "side",
                            PlaybackPauseBoundary::Disc => "disc",
                        });
                        columns.side_label = Some(&prompt.side_label);
                        columns.side_resumes_at = prompt
                            .countdown
                            .as_ref()
                            .map(|countdown| countdown.resumes_at.timestamp_millis());
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
    device_state: Option<&'static str>,
}

impl<'a> FailureColumns<'a> {
    fn of(reason: &'a PlaybackErrorReason) -> Self {
        let columns = |failure| Self {
            failure,
            problem: None,
            detail: None,
            device: None,
            device_state: None,
        };
        match reason {
            PlaybackErrorReason::SyncDisconnected => columns("sync_disconnected"),
            PlaybackErrorReason::OnOtherDevice(device) => {
                let (name, state) = match device {
                    bae_records::OtherDevice::Active { name } => (name, "active"),
                    bae_records::OtherDevice::Former { name, state } => (name, state.state_text()),
                };
                Self { device: Some(name), device_state: Some(state), ..columns("on_other_device") }
            }
            PlaybackErrorReason::OriginalMissing => columns("original_missing"),
            PlaybackErrorReason::AudioOutput { problem } => Self {
                problem: Some(match problem {
                    AudioOutputProblem::DeviceUnavailable => "device_unavailable",
                    AudioOutputProblem::UnsupportedFormat => "unsupported_format",
                    AudioOutputProblem::AirPlayRefused => "airplay_refused",
                }),
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
fn unknown(row: &Row<'_>, column: &str, value: &str) -> rusqlite::Error {
    column_conversion_error(row, column, format!("playback_now.{column} holds {value:?}"))
}

/// The row's track state, apart from the track's display.
fn state_from_row(row: &Row<'_>) -> rusqlite::Result<PlaybackState> {
    let state: String = row.get("state")?;
    if state == "stopped" {
        return Ok(PlaybackState::Stopped);
    }
    let track_id: String = row.get("track_id")?;
    let duration_ms: Option<i64> = row.get("duration_ms")?;
    let track = PlayingTrack { track_id, duration_ms: duration_ms.map(|duration| duration as u64) };
    Ok(match state.as_str() {
        "loading" => PlaybackState::Loading {
            track: if row.get::<_, bool>("prepared")? {
                LoadingTrack::Prepared(track)
            } else {
                LoadingTrack::Unprepared { track_id: track.track_id }
            },
        },
        "playing" => PlaybackState::Playing { track },
        "paused" => {
            let reason: String = row.get("pause_reason")?;
            let reason = match reason.as_str() {
                "manual" => PlaybackPauseReason::Manual,
                "side_ended" => {
                    let kind: String = row.get("side_boundary_kind")?;
                    let resumes_at: Option<i64> = row.get("side_resumes_at")?;
                    PlaybackPauseReason::SideEnded(PlaybackSidePausePrompt {
                        id: row.get("side_boundary_id")?,
                        boundary: match kind.as_str() {
                            "side" => PlaybackPauseBoundary::Side,
                            "disc" => PlaybackPauseBoundary::Disc,
                            other => return Err(unknown(row, "side_boundary_kind", other)),
                        },
                        side_label: row.get("side_label")?,
                        countdown: resumes_at
                            .map(|millis| {
                                chrono::DateTime::from_timestamp_millis(millis)
                                    .map(|resumes_at| PlaybackSideCountdown { resumes_at })
                                    .ok_or_else(|| {
                                        unknown(row, "side_resumes_at", &millis.to_string())
                                    })
                            })
                            .transpose()?,
                    })
                }
                other => return Err(unknown(row, "pause_reason", other)),
            };
            PlaybackState::Paused { track, reason }
        }
        other => return Err(unknown(row, "state", other)),
    })
}

/// The receiver a row playback is on, or lost the link to, names in its
/// `cast_receiver_*` columns.
pub(crate) fn remote_receiver_from_row(row: &Row<'_>) -> rusqlite::Result<CastReceiver> {
    Ok(CastReceiver {
        id: row.get("cast_receiver_id")?,
        name: row.get("cast_receiver_name")?,
        kind: kind_from_column(row, "cast_receiver_kind")?,
        model: row.get("cast_receiver_model")?,
        address: row.get("cast_receiver_address")?,
        takes_volume: row.get("cast_receiver_takes_volume")?,
    })
}

fn remote_from_row(row: &Row<'_>) -> rusqlite::Result<RemotePlayback> {
    let remote: String = row.get("remote")?;
    if remote == "local" {
        return Ok(RemotePlayback::Local);
    }
    let device = remote_receiver_from_row(row)?;
    Ok(match remote.as_str() {
        "on" => RemotePlayback::On(device),
        "ended" => RemotePlayback::Ended { device, detail: row.get("remote_end_detail")? },
        other => return Err(unknown(row, "remote", other)),
    })
}

fn preview_from_row(row: &Row<'_>) -> rusqlite::Result<PreviewState> {
    let preview: String = row.get("preview")?;
    if preview == "idle" {
        return Ok(PreviewState::Idle);
    }
    let target =
        preview_target_from_row(row)?.ok_or_else(|| unknown(row, "preview_path", "NULL"))?;
    let duration_ms: u64 = row.get("preview_duration_ms")?;
    Ok(match preview.as_str() {
        "playing" => PreviewState::Playing { target, duration_ms },
        "paused" => PreviewState::Paused { target, duration_ms },
        other => return Err(unknown(row, "preview", other)),
    })
}

fn failure_from_row(row: &Row<'_>) -> rusqlite::Result<Option<PlaybackFailure>> {
    let failure: Option<String> = row.get("failure")?;
    let Some(failure) = failure else {
        return Ok(None);
    };
    let diagnostic = |failure| -> rusqlite::Result<PlaybackErrorReason> {
        Ok(PlaybackErrorReason::Diagnostic { failure, detail: row.get("failure_detail")? })
    };
    let reason = match failure.as_str() {
        "sync_disconnected" => PlaybackErrorReason::SyncDisconnected,
        "on_other_device" => {
            let name: String = row.get("failure_device")?;
            let state: String = row.get("failure_device_state")?;
            PlaybackErrorReason::OnOtherDevice(match state.as_str() {
                "active" => bae_records::OtherDevice::Active { name },
                other => match bae_store::FormerDeviceState::of_state(other) {
                    Some(state) => bae_records::OtherDevice::Former { name, state },
                    None => return Err(unknown(row, "failure_device_state", other)),
                },
            })
        }
        "original_missing" => PlaybackErrorReason::OriginalMissing,
        "audio_output" => {
            let problem: String = row.get("failure_problem")?;
            PlaybackErrorReason::AudioOutput {
                problem: match problem.as_str() {
                    "device_unavailable" => AudioOutputProblem::DeviceUnavailable,
                    "unsupported_format" => AudioOutputProblem::UnsupportedFormat,
                    "airplay_refused" => AudioOutputProblem::AirPlayRefused,
                    other => return Err(unknown(row, "failure_problem", other)),
                },
            }
        }
        "internal" => diagnostic(PlaybackDiagnostic::Internal)?,
        "renderer" => diagnostic(PlaybackDiagnostic::Renderer)?,
        other => return Err(unknown(row, "failure", other)),
    };
    let number: i64 = row.get("failures")?;
    Ok(Some(PlaybackFailure { number: number as u64, reason }))
}

/// The row as the screens show it, with the queue's repeat mode and the
/// playing track's current display; `None` while the row names a track the
/// library no longer holds.
fn now_playing_on(sql: &SqlReadContext<'_>) -> Result<Option<NowPlayingValues>, DbError> {
    let (values, repeat_mode, failure) =
        sql.query_row("SELECT n.*, q.repeat FROM playback_now n, playback_queue q", [], |row| {
            Ok((
                PlaybackValues {
                    state: state_from_row(row)?,
                    volume: row.get("volume")?,
                    is_muted: row.get("is_muted")?,
                    remote: remote_from_row(row)?,
                    preview: preview_from_row(row)?,
                },
                repeat_from_row(row, "repeat")?,
                failure_from_row(row)?,
            ))
        })?;
    // `Err(None)`: the library no longer holds the track.
    let playback = values.try_map_track(|track| match track_display_on(sql, &track.track_id) {
        Ok(Some(display)) => Ok(NowPlayingTrack { track, display }),
        Ok(None) => Err(None),
        Err(error) => Err(Some(error)),
    });
    match playback {
        Ok(playback) => Ok(Some(NowPlayingValues { playback, repeat_mode, failure })),
        Err(None) => Ok(None),
        Err(Some(error)) => Err(error),
    }
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
