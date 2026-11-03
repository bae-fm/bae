//! The device's `playback_now` row: what playback is doing, written as each
//! of its values changes, and read by the screens' live query with the
//! playing track's display and the queue's repeat mode.

use super::queue::repeat_from_column;
use super::track_display::track_display_on;
use super::*;
use crate::{AudioOutputProblem, PlaybackDiagnostic, PlaybackErrorReason};
use crate::{
    LoadingTrack, NowPlayingTrack, NowPlayingValues, PlaybackChange, PlaybackFailure,
    PlaybackPauseBoundary, PlaybackPauseReason, PlaybackSideCountdown, PlaybackSidePausePrompt,
    PlaybackState, PlaybackValues, PlayingTrack, PreviewState, RemoteDevice, RemotePlayback,
};
use bae_track_audio::PreviewTarget;

/// The `playback_now` row, as playback writes it.
#[derive(Clone)]
pub struct PlaybackNowRow {
    db: Database,
}

impl PlaybackNowRow {
    /// The row over `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

/// Follow what playback is doing as its screens show it. `None` while the
/// row names a track the library no longer holds: the track was deleted, and
/// playback has yet to move off it.
pub fn subscribe_now_playing(database: &Database) -> coven::LiveQuery<Option<NowPlayingValues>> {
    database.subscribe(|sql| now_playing_on(&sql).map_err(CovenError::from))
}

/// What playback is doing now, as `subscribe_now_playing` reads it.
#[cfg(any(test, feature = "test-utils"))]
pub async fn now_playing(database: &Database) -> Result<Option<NowPlayingValues>, DbError> {
    database.read(|sql| now_playing_on(&sql)).await
}

/// The volume and mute the row holds, which resuming playback restores.
pub async fn playback_level(database: &Database) -> Result<(f32, bool), DbError> {
    database
        .read(|sql| {
            Ok(sql.query_row("SELECT volume, is_muted FROM playback_now", [], |row| {
                Ok((row.get("volume")?, row.get("is_muted")?))
            })?)
        })
        .await
}

impl PlaybackNowRow {
    /// Write `change` into the row. A state that plays clears the last
    /// failure; a failure counts up the row's failures.
    pub async fn apply(&self, change: &PlaybackChange) -> Result<(), DbError> {
        let change = change.clone();
        self.db.call(move |sql| apply_on(sql, &change).map_err(DbError::from)).await
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
                     SET failure = NULL, failure_problem = NULL, failure_detail = NULL \
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
                "UPDATE playback_now SET remote = ?, remote_device_id = ?, \
                     remote_device_name = ?, remote_end_detail = ?",
                params![
                    kind,
                    device.map(|device| device.id.as_str()),
                    device.map(|device| device.name.as_str()),
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
                let (path, start_sample, end_sample) = target.clone().into_sample_range();
                (
                    coven::path_bytes(&path),
                    start_sample as i64,
                    end_sample.map(|end| end as i64),
                    *duration_ms as i64,
                )
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
            let (failure, problem, detail) = failure_columns(reason);
            sql.execute(
                "UPDATE playback_now SET failure = ?, failure_problem = ?, failure_detail = ?, \
                     failures = failures + 1",
                params![failure, problem, detail],
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
             remote = 'local', remote_device_id = NULL, remote_device_name = NULL, \
             remote_end_detail = NULL, preview = 'idle', preview_path = NULL, \
             preview_start_sample = NULL, preview_end_sample = NULL, preview_duration_ms = NULL, \
             failure = NULL, failure_problem = NULL, failure_detail = NULL, failures = 0",
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
fn failure_columns(
    reason: &PlaybackErrorReason,
) -> (&'static str, Option<&'static str>, Option<&str>) {
    match reason {
        PlaybackErrorReason::SyncDisconnected => ("sync_disconnected", None, None),
        PlaybackErrorReason::NotUploaded => ("not_uploaded", None, None),
        PlaybackErrorReason::OriginalMissing => ("original_missing", None, None),
        PlaybackErrorReason::AudioOutput { problem } => (
            "audio_output",
            Some(match problem {
                AudioOutputProblem::DeviceUnavailable => "device_unavailable",
                AudioOutputProblem::UnsupportedFormat => "unsupported_format",
                AudioOutputProblem::AirPlayRefused => "airplay_refused",
            }),
            None,
        ),
        PlaybackErrorReason::Diagnostic { failure, detail } => (
            match failure {
                PlaybackDiagnostic::Internal => "internal",
                PlaybackDiagnostic::Renderer => "renderer",
            },
            None,
            Some(detail),
        ),
    }
}

/// A column value the schema's CHECKs rule out: the row is one nothing here
/// wrote.
fn unknown(column: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        format!("playback_now.{column} holds {value:?}").into(),
    )
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
                            other => return Err(unknown("side_boundary_kind", other)),
                        },
                        side_label: row.get("side_label")?,
                        countdown: resumes_at
                            .map(|millis| {
                                chrono::DateTime::from_timestamp_millis(millis)
                                    .map(|resumes_at| PlaybackSideCountdown { resumes_at })
                                    .ok_or_else(|| unknown("side_resumes_at", &millis.to_string()))
                            })
                            .transpose()?,
                    })
                }
                other => return Err(unknown("pause_reason", other)),
            };
            PlaybackState::Paused { track, reason }
        }
        other => return Err(unknown("state", other)),
    })
}

fn remote_from_row(row: &Row<'_>) -> rusqlite::Result<RemotePlayback> {
    let remote: String = row.get("remote")?;
    if remote == "local" {
        return Ok(RemotePlayback::Local);
    }
    let device =
        RemoteDevice { id: row.get("remote_device_id")?, name: row.get("remote_device_name")? };
    Ok(match remote.as_str() {
        "on" => RemotePlayback::On(device),
        "ended" => RemotePlayback::Ended { device, detail: row.get("remote_end_detail")? },
        other => return Err(unknown("remote", other)),
    })
}

fn preview_from_row(row: &Row<'_>) -> rusqlite::Result<PreviewState> {
    let preview: String = row.get("preview")?;
    if preview == "idle" {
        return Ok(PreviewState::Idle);
    }
    let path = coven::path_from_bytes(row.get("preview_path")?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Blob,
            format!("playback_now.preview_path does not open on this device: {error}").into(),
        )
    })?;
    let start_sample: i64 = row.get("preview_start_sample")?;
    let end_sample: Option<i64> = row.get("preview_end_sample")?;
    let duration_ms: i64 = row.get("preview_duration_ms")?;
    let target =
        PreviewTarget::sample_range(path, start_sample as u64, end_sample.map(|end| end as u64));
    let duration_ms = duration_ms as u64;
    Ok(match preview.as_str() {
        "playing" => PreviewState::Playing { target, duration_ms },
        "paused" => PreviewState::Paused { target, duration_ms },
        other => return Err(unknown("preview", other)),
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
        "not_uploaded" => PlaybackErrorReason::NotUploaded,
        "original_missing" => PlaybackErrorReason::OriginalMissing,
        "audio_output" => {
            let problem: String = row.get("failure_problem")?;
            PlaybackErrorReason::AudioOutput {
                problem: match problem.as_str() {
                    "device_unavailable" => AudioOutputProblem::DeviceUnavailable,
                    "unsupported_format" => AudioOutputProblem::UnsupportedFormat,
                    "airplay_refused" => AudioOutputProblem::AirPlayRefused,
                    other => return Err(unknown("failure_problem", other)),
                },
            }
        }
        "internal" => diagnostic(PlaybackDiagnostic::Internal)?,
        "renderer" => diagnostic(PlaybackDiagnostic::Renderer)?,
        other => return Err(unknown("failure", other)),
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
            let repeat: String = row.get("repeat")?;
            Ok((
                PlaybackValues {
                    state: state_from_row(row)?,
                    volume: row.get("volume")?,
                    is_muted: row.get("is_muted")?,
                    remote: remote_from_row(row)?,
                    preview: preview_from_row(row)?,
                },
                repeat_from_column(&repeat)?,
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

#[cfg(test)]
#[path = "now_tests.rs"]
mod tests;
