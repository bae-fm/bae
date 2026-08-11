import Combine
import SwiftUI
import os.log

private let logger = Logger.bae("PlaybackStore")

/// Mirror of core's playback state. The event dispatcher is the sole writer:
/// `nowPlaying`, `volume`, `isMuted`, `repeatMode`, `manualQueue`, and
/// `queueContext` are all driven by `BridgeUiEvent` deliveries. Views read
/// fields at the leaf and never write back — they invoke `appHandle` actions
/// instead, and the resulting events flow back through the dispatcher.
///
/// Not `@MainActor` on the whole type: `MediaControlService.handleScrub` calls
/// `projectSeek` from a nonisolated remote-command callback. Only
/// `loadUpcomingRange`/its private helper are `@MainActor` (see their doc
/// comments) since they spawn a `Task` capturing `self`, which Swift 6 only
/// allows across a `Task` boundary when the capture is actor-isolated.
@Observable
public class PlaybackStore {
    public var nowPlaying: NowPlaying = .stopped

    public var volume: Float = 1.0
    public var isMuted: Bool = false
    public var repeatMode: BridgeRepeatMode = .off
    /// The manual lane ("Up Next") — explicitly enqueued tracks, drained first.
    public var manualQueue: [QueueItem] = []
    /// The context (the release being played from), or `nil` when nothing plays
    /// from a release. Rendered as a section distinct from `manualQueue`.
    /// `context.upcoming` is only the initial window; further indices, once
    /// fetched via `loadUpcomingRange`, live in `pagedUpcoming` — read either
    /// through `upcomingItem(at:)`.
    public var queueContext: QueuePlaybackContext?
    /// Context-tail entries fetched past the initial window, keyed by their
    /// absolute index in the tail. Reset wholesale on every `applyQueueSnapshot`
    /// — a `QueueUpdated` is the invalidation signal for this ephemeral view
    /// cache, not durable state to reconcile incrementally.
    public var pagedUpcoming: [Int: QueueItem] = [:]
    /// The queue revision the current `manualQueue`/`queueContext` were resolved
    /// from. Stamped onto every `loadUpcomingRange` fetch so a reply computed
    /// under a since-superseded revision is dropped rather than merged.
    @ObservationIgnored
    public private(set) var revision: UInt64 = 0
    /// In-flight `loadUpcomingRange` fetches, keyed "offset:end:revision", so
    /// concurrent callers asking for the same range (every row in a batch
    /// window mounting at once) coalesce onto one bridge call instead of each
    /// issuing a duplicate.
    @ObservationIgnored
    private var inFlightUpcomingLoads: [String: Task<Void, Never>] = [:]

    /// Current playback position. Updates at display rate during playback —
    /// far too frequent for `@Observable`; published as a Combine signal so
    /// only the progress-bar NSView re-renders.
    @ObservationIgnored
    private let playbackPositionSubject = CurrentValueSubject<
        PlaybackPositionEvent, Never
    >(.reset)
    private var playbackPosition: PlaybackPositionState?

    /// Fires when tracks have been appended/inserted into the playback queue.
    /// Payload is the count from a single add operation (release add, drag,
    /// add-next, etc.). Drives the transient "+N" badge on the queue button.
    /// One-shot signal — view-local state holds the displayed count and a
    /// fade timer.
    @ObservationIgnored
    private let queueItemsAddedSubject = PassthroughSubject<Int, Never>()

    public var playbackPositionPublisher:
        AnyPublisher<PlaybackPositionEvent, Never>
    {
        playbackPositionSubject.eraseToAnyPublisher()
    }

    public var playbackPositionEvent: PlaybackPositionEvent {
        playbackPositionSubject.value
    }

    public var queueItemsAddedPublisher: AnyPublisher<Int, Never> {
        queueItemsAddedSubject.eraseToAnyPublisher()
    }

    public init() {}

    public func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        guard snapshot.revision >= revision else {
            logger.debug(
                "dropping queue snapshot at revision \(snapshot.revision); revision \(self.revision) is already applied"
            )
            return
        }
        manualQueue = snapshot.manual.map(QueueItem.init(bridge:))
        queueContext = snapshot.context.map(QueuePlaybackContext.init(bridge:))
        revision = snapshot.revision
        // The event is the invalidation signal: every previously fetched page
        // is dropped, not reconciled against the new snapshot.
        pagedUpcoming = [:]
    }

    /// The context-tail item at absolute `index`, or `nil` if not yet loaded —
    /// either still outside the initial window and not yet paged in, or past
    /// `upcomingTotal` entirely.
    public func upcomingItem(at index: Int) -> QueueItem? {
        guard let context = queueContext else {
            return nil
        }
        if index < context.upcoming.count {
            return context.upcoming[index]
        }
        return pagedUpcoming[index]
    }

    /// Fetch `[offset, offset + limit)` of the context's upcoming tail and
    /// merge it into `pagedUpcoming`. A no-op if the range is already loaded.
    /// The reply is applied only if its revision still matches this store's
    /// current `revision`; a mismatch means a `QueueUpdated` for a newer queue
    /// state arrived while the fetch was in flight, and that event's wholesale
    /// reset already replaced the view, so the reply is dropped rather than
    /// merged. Fetch failures are never swallowed silently: logged at warn with
    /// the failed range — a page fetch is background prefetch, not a
    /// user-initiated action with its own error-display path.
    ///
    /// `@MainActor`: spawns a `Task` capturing `self` to coalesce concurrent
    /// range fetches; Swift 6 only allows that capture across a `Task`
    /// boundary when the call site (and the spawned task, which inherits it)
    /// is actor-isolated. Callers are SwiftUI `.task(id:)` bodies, already on
    /// the main actor.
    @MainActor
    public func loadUpcomingRange(offset: Int, limit: Int, queue: Queue) async {
        guard let context = queueContext else {
            return
        }
        let end = min(offset + limit, context.upcomingTotal)
        guard offset < end else {
            return
        }
        guard !(offset..<end).allSatisfy({ upcomingItem(at: $0) != nil })
        else {
            return
        }

        let key = "\(offset):\(end):\(revision)"
        if let existing = inFlightUpcomingLoads[key] {
            await existing.value
            return
        }
        let task = Task {
            await self.fetchUpcomingRange(
                offset: offset,
                end: end,
                queue: queue
            )
        }
        inFlightUpcomingLoads[key] = task
        await task.value
        inFlightUpcomingLoads[key] = nil
    }

    @MainActor
    private func fetchUpcomingRange(offset: Int, end: Int, queue: Queue) async {
        do {
            let page = try await queue.getUpcomingPage(
                UInt32(offset),
                UInt32(end - offset)
            )
            guard page.revision == revision else {
                logger.warning(
                    "dropping upcoming page for [\(offset), \(end)): fetched under a since-superseded revision"
                )
                return
            }
            for (i, entry) in page.entries.enumerated() {
                pagedUpcoming[offset + i] = QueueItem(bridge: entry)
            }
        }
        catch {
            logger.warning(
                "failed to load upcoming range [\(offset), \(end)): \(error.localizedDescription)"
            )
        }
    }

    /// Enter the loading transition for `trackId`, retaining the currently
    /// displayed track. Core's first `PlaybackLoading` carries only a track id
    /// (before it resolves metadata); without carrying the prior track forward,
    /// `nowPlaying.track` would go nil on every transition and tear down the
    /// now-playing bar — which on iOS also dismisses the expanded full-screen
    /// player. The prior track stays on screen until the target's metadata
    /// lands via `setLoadingTarget`.
    public func beginLoading(trackId: String) {
        let previousTrackId = nowPlaying.track?.trackId
        nowPlaying = .loading(
            trackId: trackId,
            target: nil,
            previous: nowPlaying.track
        )
        if trackId != previousTrackId {
            resetPlaybackPosition()
        }
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
    public func setLoadingTarget(trackId: String, target: NowPlayingTrack) {
        // The track to keep on screen behind the spinner: the prior loading's
        // fallback when already loading this track, or the current track when a
        // seek re-enters loading from playing/paused.
        let previous: NowPlayingTrack?
        switch nowPlaying {
        case .loading(trackId, _, let priorFallback):
            previous = priorFallback
        case .playing(let current) where current.trackId == trackId,
            .paused(let current, _) where current.trackId == trackId:
            previous = current
        default:
            // A fast switch moved on: the resolved target is for a track that is
            // no longer current. Dropping it is correct — the newer load owns
            // the now-playing bar — but record it so a stuck bar is diagnosable.
            logger.debug(
                "dropping stale loading target for \(trackId); no longer the current track"
            )
            return
        }
        nowPlaying = .loading(
            trackId: trackId,
            target: target,
            previous: previous
        )
    }

    public func pause(track: NowPlayingTrack, reason: BridgePlaybackPauseReason)
    {
        preparePlaybackPosition(for: track)
        nowPlaying = .paused(track, reason: reason)
    }

    public func play(track: NowPlayingTrack) {
        preparePlaybackPosition(for: track)
        nowPlaying = .playing(track)
    }

    public func updatePlaybackProgress(
        trackId: String,
        positionMs: UInt64,
        durationMs: UInt64,
        progress: Double
    ) -> PlaybackPositionSnapshot? {
        guard acceptsPlaybackPosition(trackId: trackId) else {
            return playbackPosition?.snapshot
        }
        if case .projected(let projected, let pendingSeek) = playbackPosition {
            if pendingSeek.matches(trackId: trackId) {
                return projected
            }
        }
        return publishCurrentPosition(
            positionMs: positionMs,
            durationMs: durationMs,
            progress: progress
        )
    }

    public func updatePlaybackSeeked(
        trackId: String,
        positionMs: UInt64,
        durationMs: UInt64,
        progress: Double
    ) -> PlaybackPositionSnapshot? {
        guard acceptsPlaybackPosition(trackId: trackId) else {
            return playbackPosition?.snapshot
        }
        return publishCurrentPosition(
            positionMs: positionMs,
            durationMs: durationMs,
            progress: progress
        )
    }

    private func publishCurrentPosition(
        positionMs: UInt64,
        durationMs: UInt64,
        progress: Double
    ) -> PlaybackPositionSnapshot {
        let snapshot = PlaybackPositionSnapshot(
            positionMs: positionMs,
            durationMs: durationMs,
            progress: progress
        )
        return publish(snapshot, state: .current(snapshot))
    }

    private func acceptsPlaybackPosition(trackId: String) -> Bool {
        guard let currentTrackId = nowPlaying.track?.trackId,
            currentTrackId != trackId
        else {
            return true
        }
        logger.warning(
            "ignoring playback position for stale track \(trackId); current track is \(currentTrackId)"
        )
        return false
    }

    @discardableResult
    public func projectSeek(ratio: Double) -> PlaybackPositionSnapshot? {
        guard let currentPosition = playbackPosition?.snapshot,
            currentPosition.durationMs > 0
        else {
            logger.warning(
                "Seek projection ignored for ratio \(ratio): no known playback duration"
            )
            return nil
        }
        let clampedRatio = min(1.0, max(0.0, ratio))
        let targetPositionMs = UInt64(
            (clampedRatio * Double(currentPosition.durationMs)).rounded()
        )
        let snapshot = PlaybackPositionSnapshot(
            positionMs: targetPositionMs,
            durationMs: currentPosition.durationMs,
            progress: clampedRatio
        )
        let pendingSeek = PendingSeek(
            trackId: nowPlaying.track?.trackId
        )
        return publish(snapshot, state: .projected(snapshot, pendingSeek))
    }

    public func resetPlaybackPosition() {
        playbackPosition = nil
        playbackPositionSubject.send(.reset)
    }

    func publishQueueItemsAdded(_ count: Int) {
        queueItemsAddedSubject.send(count)
    }

    private func preparePlaybackPosition(for track: NowPlayingTrack) {
        if track.trackId != nowPlaying.track?.trackId {
            playbackPosition = playbackPosition?.withoutProjection
        }
    }

    private func publish(
        _ snapshot: PlaybackPositionSnapshot,
        state: PlaybackPositionState
    ) -> PlaybackPositionSnapshot {
        playbackPosition = state
        playbackPositionSubject.send(snapshot.event)
        return snapshot
    }
}

private enum PlaybackPositionState {
    case current(PlaybackPositionSnapshot)
    case projected(PlaybackPositionSnapshot, PendingSeek)

    var snapshot: PlaybackPositionSnapshot {
        switch self {
        case .current(let snapshot), .projected(let snapshot, _):
            snapshot
        }
    }

    var withoutProjection: PlaybackPositionState {
        .current(snapshot)
    }
}

private struct PendingSeek {
    let trackId: String?

    func matches(trackId: String) -> Bool {
        self.trackId == nil || self.trackId == trackId
    }
}

// ── NowPlaying ─────────────────────────────────────────────────────────

public struct NowPlayingTrack {
    public let trackId: String
    public let trackTitle: String
    public let artistNames: String
    public let albumId: String
    public let coverImage: BridgeImageRef?
    public let durationMs: UInt64

    public init(
        trackId: String,
        trackTitle: String,
        artistNames: String,
        albumId: String,
        coverImage: BridgeImageRef?,
        durationMs: UInt64
    ) {
        self.trackId = trackId
        self.trackTitle = trackTitle
        self.artistNames = artistNames
        self.albumId = albumId
        self.coverImage = coverImage
        self.durationMs = durationMs
    }
}

extension BridgePlaybackPauseReason {
    public var sidePausePrompt: BridgeSidePausePrompt? {
        guard case .sideEnded(prompt: let prompt) = self else {
            return nil
        }
        return prompt
    }
}

extension BridgeSidePausePrompt: Identifiable {
    public func title() -> String {
        String(
            format: localizedCoreString(titleKey),
            sideLetter
        )
    }

    public func message() -> String {
        localizedCoreString(messageKey)
    }
}

public enum NowPlaying {
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
    case paused(NowPlayingTrack, reason: BridgePlaybackPauseReason)

    public var isActive: Bool {
        if case .stopped = self {
            return false
        }
        return true
    }

    public var track: NowPlayingTrack? {
        switch self {
        case .playing(let t), .paused(let t, _): t
        case .loading(_, let target, let previous): target ?? previous
        case .stopped: nil
        }
    }

    public var secondaryLine: String? {
        switch self {
        case .paused(let track, let reason):
            if let prompt = reason.sidePausePrompt {
                return prompt.title()
            }
            return track.artistNames
        case .playing(let track):
            return track.artistNames
        case .loading(_, let target, let previous):
            if let target {
                return target.artistNames
            }
            if let previous {
                return previous.artistNames
            }
            return nil
        case .stopped:
            return nil
        }
    }

    /// The id of the track currently loading, or `nil` when not loading. Lets
    /// track rows / the transport mark exactly the loading track without
    /// confusing it with the displayed (possibly still-previous) track.
    public var loadingTrackId: String? {
        switch self {
        case .loading(let trackId, _, _): trackId
        case .playing, .paused, .stopped: nil
        }
    }

    /// True while a track is playing or being prepared to play. `.loading` is a
    /// play-intent transition (auto-advance / skip / initial play all resolve
    /// to `.playing`), so the transport shows the pause glyph through the gap
    /// instead of flickering to the play glyph and back.
    public var isPlaying: Bool {
        switch self {
        case .playing, .loading: true
        case .paused, .stopped: false
        }
    }
}
