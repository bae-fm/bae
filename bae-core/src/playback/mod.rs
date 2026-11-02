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
mod repeat_mode;
mod resampling_drain;
pub mod service;
pub mod source;
pub mod stream_pipeline;
pub mod timeline;
mod track_buffered;

pub use audio_focus::{AudioFocus, AudioFocusRefused, AudioFocusSystem};
pub use audio_output::{
    AudioError, AudioOutput, AudioOutputDevice, AudioState, AudioStream, CompletionEvent,
    PositionEvent,
};
#[cfg(feature = "test-utils")]
pub use audio_output::{
    CaptureAudioDevice, CaptureAudioOutput, CapturedStream, CapturedStreams,
    RealtimeCaptureAudioDevice, RealtimeCaptureAudioOutput, RealtimeProbeDevice,
    RealtimeProbeOutput,
};
#[cfg(any(test, feature = "test-utils"))]
pub use clock::ManualPlaybackClock;
pub use clock::WallPlaybackClock;
pub use clock::{PlaybackClock, PlaybackClockRef, PlaybackSleep};
pub use context::ContextSource;
pub(crate) use error::read_failure_reason;
pub use error::PlaybackError;
pub use error_reason::{AudioOutputProblem, PlaybackDiagnostic, PlaybackErrorReason};
pub use now_playing::{
    NowPlayingTrack, NowPlayingValues, PlaybackFailure, PlayingTrack, TrackDisplay,
};
pub use progress::{
    MediaControlPlayback, PlaybackChange, PlaybackPosition, PlaybackPositions, PlaybackProgress,
    PlaybackTimeline, PlaybackValues, PreviewPosition, PreviewState, PreviewTimelinePoint,
    RemoteDevice, RemotePlayback, TimelinePoint,
};
pub use queue::{NextEntry, PlaybackQueue, PreviousAction, QueueEntry, QueueEntryId};
pub use repeat_mode::RepeatMode;
pub use service::{
    LoadingTrack, PlaybackHandle, PlaybackPauseBoundary, PlaybackPauseReason, PlaybackService,
    PlaybackSideCountdown, PlaybackSidePausePrompt, PlaybackState, PlaybackTrackInfo,
    PlaybackTrackSide,
};
pub use source::{TrackCrossing, TrackFmt};
pub use timeline::{StreamPosition, TrackTimeline};
pub use track_buffered::{TrackBuffered, TrackBufferedFollow, TrackSpan};
