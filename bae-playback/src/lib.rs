//! Playback: the service that plays the queue on this device's output or a
//! remote renderer, the queue and its resume point as the store keeps them,
//! the outputs it plays on, the preview player, and the renderer session a
//! connected cast or UPnP device is driven through.
#[cfg(target_os = "android")]
pub mod aaudio_output;
pub mod airplay_output;
mod audio_focus;
pub mod audio_output;
mod clock;
mod context;
#[cfg(not(target_os = "android"))]
pub mod cpal_output;
// The formats bae puts Mac output devices back to; its tests run on any host.
#[cfg(any(target_os = "macos", test))]
mod device_formats;
// The blocking-write device writer is AAudio's; its tests run on the host.
#[cfg(any(target_os = "android", test))]
mod device_writer;
mod error;
mod error_reason;
mod now_playing;
#[cfg(any(target_os = "macos", test))]
mod output_rate;
mod preview_player;
pub mod progress;
mod queue;
mod queue_values;
pub mod renderer;
mod repeat_mode;
mod replay_gain;
mod resampling_drain;
pub mod service;
pub mod source;
pub mod stream_pipeline;
pub mod tables;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_capture;
pub mod timeline;
mod track_buffered;
mod volume_level;

pub use audio_focus::{AudioFocus, AudioFocusRefused, AudioFocusSystem};
pub use audio_output::{
    AudioError, AudioOutput, AudioOutputDevice, AudioState, AudioStream, CompletionEvent,
    PositionEvent,
};
#[cfg(any(test, feature = "test-utils"))]
pub use audio_output::{
    CaptureAudioDevice, CaptureAudioOutput, CapturedStream, CapturedStreamSender, CapturedStreams,
    RealtimeCaptureAudioDevice, RealtimeCaptureAudioOutput, RealtimeProbeDevice,
    RealtimeProbeOutput,
};
#[cfg(any(test, feature = "test-utils"))]
pub use clock::ManualPlaybackClock;
pub use clock::WallPlaybackClock;
pub use clock::{PlaybackClock, PlaybackClockRef, PlaybackSleep};
pub use context::ContextSource;
pub use error::read_failure_reason;
pub use error::PlaybackError;
pub use error_reason::{AudioOutputProblem, PlaybackDiagnostic, PlaybackErrorReason};
pub use now_playing::{
    NowPlayingTrack, NowPlayingValues, PlaybackFailure, PlayingTrack, TrackDisplay,
};
pub use progress::{
    MediaControlPlayback, PlaybackChange, PlaybackPosition, PlaybackPositions,
    PlaybackPositionsWatch, PlaybackProgress, PlaybackProgressFeed, PlaybackTimeline,
    PlaybackValues, PreviewPosition, PreviewState, PreviewTimelinePoint, RemoteDevice,
    RemotePlayback, TimelinePoint,
};
pub use queue::{NextEntry, PlaybackQueue, PreviousAction, QueueEntry, QueueEntryId};
pub use queue_values::{
    QueueAddition, QueueItem, QueueUpcomingWindow, ResolvedContext, ResolvedQueueSnapshot,
};
pub use repeat_mode::RepeatMode;
pub use replay_gain::replay_gain_linear;
pub use service::{
    LoadingTrack, PlayError, PlayOutcome, PlaybackHandle, PlaybackPauseBoundary,
    PlaybackPauseReason, PlaybackService, PlaybackSideCountdown, PlaybackSidePausePrompt,
    PlaybackState, PlaybackTrackInfo, PlaybackTrackSide, QueueEditError,
};
pub use source::{TrackCrossing, TrackFmt};
pub use timeline::{StreamPosition, TrackTimeline};
pub use track_buffered::{TrackBuffered, TrackBufferedFollow, TrackSpan};
pub use volume_level::Volume;
