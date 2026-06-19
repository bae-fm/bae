import Combine
import SwiftUI
import os.log

private let logger = Logger.bae("PlaybackStore")

/// Mirror of core's playback state. The reducer is the sole writer:
/// `nowPlaying`, `volume`, `isMuted`, `repeatMode`, and `queueItems` are
/// all driven by `BridgeUiEvent` deliveries. Views read fields at the
/// leaf and never write back — they invoke `appHandle` actions instead,
/// and the resulting events flow back through the reducer.
@Observable
class PlaybackStore {
    var nowPlaying: NowPlaying = .stopped

    var volume: Float = 1.0
    var isMuted: Bool = false
    var repeatMode: RepeatMode = .none
    var queueItems: [QueueItem] = []

    /// Current playback position. Updates at display rate during playback —
    /// far too frequent for `@Observable`; published as a Combine signal so
    /// only the progress-bar NSView re-renders.
    @ObservationIgnored
    let playbackPositionSubject = CurrentValueSubject<
        PlaybackPositionEvent, Never
    >(.reset)

    /// Fires when tracks have been appended/inserted into the playback queue.
    /// Payload is the count from a single add operation (release add, drag,
    /// add-next, etc.). Drives the transient "+N" badge on the queue button.
    /// One-shot signal — view-local state holds the displayed count and a
    /// fade timer.
    @ObservationIgnored
    let queueItemsAddedSubject = PassthroughSubject<Int, Never>()

    /// Enter the loading transition for `trackId`, retaining the currently
    /// displayed track. Core's first `PlaybackLoading` carries only a track id
    /// (before it resolves metadata); without carrying the prior track forward,
    /// `nowPlaying.track` would go nil on every transition and tear down the
    /// now-playing bar — which on iOS also dismisses the expanded full-screen
    /// player. The prior track stays on screen until the target's metadata
    /// lands via `setLoadingTarget`.
    func beginLoading(trackId: String) {
        nowPlaying = .loading(
            trackId: trackId,
            target: nil,
            previous: nowPlaying.track
        )
    }

    /// A loading state carrying the target track's metadata arrived (core's
    /// resolved `PlaybackLoading`). Two cases enter loading:
    ///
    /// - Already loading this track (the play path's second event): swap the
    ///   displayed track from the prior one to the resolved target while audio
    ///   still downloads.
    /// - Playing or paused this same track (a seek): core re-enters loading
    ///   while it buffers the seek target, so show the spinner and keep the
    ///   current track on screen as the fallback.
    ///
    /// Any other state means a faster switch moved on to a different track; the
    /// resolved target is stale and dropped.
    func setLoadingTarget(trackId: String, target: NowPlayingTrack) {
        // The track to keep on screen behind the spinner: the prior loading's
        // fallback when already loading this track, or the current track when a
        // seek re-enters loading from playing/paused.
        let previous: NowPlayingTrack?
        switch nowPlaying {
        case .loading(trackId, _, let priorFallback):
            previous = priorFallback
        case .playing(let current) where current.trackId == trackId,
            .paused(let current) where current.trackId == trackId:
            previous = current
        default:
            // A fast switch moved on: the resolved target is for a track that is
            // no longer current. Dropping it is correct — the newer load owns
            // the now-playing bar — but record it so a stuck bar is diagnosable.
            logger.debug(
                "dropping stale loading target for \(trackId, privacy: .public); no longer the current track"
            )
            return
        }
        nowPlaying = .loading(
            trackId: trackId,
            target: target,
            previous: previous
        )
    }
}

// ── NowPlaying ─────────────────────────────────────────────────────────

struct NowPlayingTrack {
    let trackId: String
    let trackTitle: String
    let artistNames: String
    let albumId: String
    let coverImageId: String?
    let durationMs: UInt64
}

enum NowPlaying {
    case stopped
    /// A track is being prepared. `target` is the loading track's own metadata
    /// once core resolves it (`nil` until then); `previous` is whatever was on
    /// screen when the transition began. `track` shows the target the moment it
    /// lands and falls back to `previous` before that, so the now-playing UI
    /// keeps rendering (rather than going blank / tearing down) across the gap.
    case loading(
        trackId: String,
        target: NowPlayingTrack?,
        previous: NowPlayingTrack?
    )
    case playing(NowPlayingTrack)
    case paused(NowPlayingTrack)

    var isActive: Bool {
        if case .stopped = self {
            return false
        }
        return true
    }

    var track: NowPlayingTrack? {
        switch self {
        case .playing(let t), .paused(let t): t
        case .loading(_, let target, let previous): target ?? previous
        case .stopped: nil
        }
    }

    /// The id of the track currently loading, or `nil` when not loading. Lets
    /// track rows / the transport mark exactly the loading track without
    /// confusing it with the displayed (possibly still-previous) track.
    var loadingTrackId: String? {
        switch self {
        case .loading(let trackId, _, _): trackId
        case .playing, .paused, .stopped: nil
        }
    }

    /// True while a track is playing or being prepared to play. `.loading` is a
    /// play-intent transition (auto-advance / skip / initial play all resolve
    /// to `.playing`), so the transport shows the pause glyph through the gap
    /// instead of flickering to the play glyph and back.
    var isPlaying: Bool {
        switch self {
        case .playing, .loading: true
        case .paused, .stopped: false
        }
    }
}
