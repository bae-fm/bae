#[cfg(target_os = "android")]
pub mod aaudio_output;
pub mod airplay_output;
mod audio_focus;
pub mod audio_output;
mod clock;
mod context;
#[cfg(not(target_os = "android"))]
pub mod cpal_output;
// The blocking-write device writer is AAudio's; its tests run on the host.
pub mod data_source;
#[cfg(any(target_os = "android", test))]
mod device_writer;
mod error;
mod now_playing;
#[cfg(any(target_os = "macos", test))]
mod output_rate;
mod preview_player;
pub mod progress;
mod queue;
mod repeat_mode;
mod resampling_drain;
mod scrub_points;
pub mod service;
pub mod source;
pub mod stream_pipeline;
pub mod timeline;
// Whole-release passes (import loudness, release saves) are desktop-only.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod track_sources;

pub use audio_focus::{AudioFocus, AudioFocusRefused, AudioFocusSystem};
#[cfg(feature = "test-utils")]
pub use audio_output::{
    wait_for_samples, CaptureAudioDevice, CaptureAudioOutput, RealtimeCaptureAudioDevice,
    RealtimeCaptureAudioOutput, RealtimeProbeDevice, RealtimeProbeOutput,
};
pub use audio_output::{
    AudioError, AudioOutput, AudioOutputDevice, AudioState, AudioStream, CompletionEvent,
    PositionEvent,
};
#[cfg(any(test, feature = "test-utils"))]
pub use clock::ManualPlaybackClock;
pub(crate) use clock::WallPlaybackClock;
pub use clock::{PlaybackClock, PlaybackClockRef, PlaybackSleep};
pub use context::ContextSource;
pub use error::PlaybackError;
pub use now_playing::{NowPlayingTrack, NowPlayingValues, PlayingTrack, TrackDisplay};
pub use preview_player::PreviewTarget;
pub use progress::{
    MediaControlPlayback, PlaybackPosition, PlaybackPositions, PlaybackProgress, PlaybackTimeline,
    PlaybackValues, PreviewPosition, PreviewState, PreviewTimelinePoint, RemoteDevice,
    RemotePlayback, TimelinePoint,
};
pub use queue::{NextEntry, PlaybackQueue, PreviousAction, QueueEntry, QueueEntryId};
pub use repeat_mode::RepeatMode;
pub use scrub_points::ScrubPoints;
pub use service::{
    LoadingTrack, PlaybackHandle, PlaybackPauseBoundary, PlaybackPauseReason, PlaybackService,
    PlaybackSideCountdown, PlaybackSidePausePrompt, PlaybackState, PlaybackTrackInfo,
    PlaybackTrackSide,
};
pub use source::{TrackCrossing, TrackFmt};
pub use timeline::{StreamPosition, TrackTime, TrackTimeline};
