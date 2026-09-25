import Combine
import SwiftUI
import os.log

private let logger = Logger.bae("PlaybackStore")
/// How many windows of the context's upcoming tail stay read at once: the one
/// around the visible rows and the two nearest it.
private let maximumUpcomingWindows = 3

/// Mirror of core's playback state. Retained value subscriptions are the writer:
/// `nowPlaying`, `volume`, `isMuted`, `repeatMode`, `manualQueue`, and
/// `queueContext` are driven by retained playback and queue values. Views read
/// fields at the leaf and never write back — they invoke `appHandle` actions
/// instead, and the resulting values flow back through their stores.
///
/// Not `@MainActor` on the whole type: `MediaControlService.handleScrub` calls
/// `projectSeek` from a nonisolated remote-command callback. Only
/// `loadUpcomingRange`/its private helper are `@MainActor` (see their doc
/// comments) since they spawn a `Task` capturing `self`, which Swift 6 only
/// allows across a `Task` boundary when the capture is actor-isolated.
@Observable
public class PlaybackStore {
    public private(set) var nowPlaying: NowPlaying = .stopped
    private var dismissedSidePausePromptId: String?

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
    /// Context-tail entries read past the initial window, keyed by their
    /// absolute index in the tail: the latest upcoming value whose revision
    /// matches `revision`, and empty while none does.
    public var pagedUpcoming: [Int: QueueItem] = [:]
    /// The queue revision the current `manualQueue`/`queueContext` were resolved
    /// from. Upcoming values sliced from any other revision are not shown.
    @ObservationIgnored
    public private(set) var revision: UInt64 = 0
    /// The one live read of the upcoming tail, opened by the first
    /// `loadUpcomingRange`. Its windows move in place as the queue scrolls,
    /// and the queue's revisions move it in core; neither opens another.
    @ObservationIgnored
    private var upcomingQuery: QueueUpcomingQuery?
    @ObservationIgnored
    private var upcomingDeliveries: Task<Void, Never>?
    /// The tail ranges the upcoming read covers, at most
    /// `maximumUpcomingWindows`; loading one past that drops the one farthest
    /// from it.
    @ObservationIgnored
    private var upcomingWindows: [Range<Int>] = []
    /// The newest upcoming value, kept until the queue value of its revision
    /// arrives when it lands first.
    @ObservationIgnored
    private var latestUpcoming: BridgeQueueUpcomingSnapshot?

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

    deinit {
        upcomingDeliveries?.cancel()
        if let upcomingQuery {
            Task { await upcomingQuery.cancel() }
        }
    }

    public var presentedSidePausePrompt: BridgeSidePausePrompt? {
        guard let prompt = nowPlaying.sidePausePrompt,
            prompt.id != dismissedSidePausePromptId
        else {
            return nil
        }
        return prompt
    }

    public func dismissSidePausePrompt(_ prompt: BridgeSidePausePrompt) {
        dismissedSidePausePromptId = prompt.id
    }

    public func stop() {
        setNowPlaying(.stopped)
        resetPlaybackPosition()
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
        setNowPlaying(
            .loading(
                trackId: trackId,
                target: nil,
                previous: nowPlaying.track
            )
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
        setNowPlaying(
            .loading(
                trackId: trackId,
                target: target,
                previous: previous
            )
        )
    }

    public func pause(track: NowPlayingTrack, reason: BridgePlaybackPauseReason)
    {
        preparePlaybackPosition(for: track)
        setNowPlaying(.paused(track, reason: reason))
    }

    public func play(track: NowPlayingTrack) {
        preparePlaybackPosition(for: track)
        setNowPlaying(.playing(track))
    }

    public func updatePlaybackProgress(
        trackId: String,
        positionMs: Int64,
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
        positionMs: Int64,
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
        positionMs: Int64,
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
        let targetPositionMs = Int64(
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

    private func setNowPlaying(_ next: NowPlaying) {
        if nowPlaying.sidePausePrompt?.id != next.sidePausePrompt?.id {
            dismissedSidePausePromptId = nil
        }
        nowPlaying = next
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

extension PlaybackStore {
    public func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        guard snapshot.revision >= revision else {
            logger.debug(
                "dropping queue snapshot at revision \(snapshot.revision); revision \(self.revision) is already applied"
            )
            return
        }
        manualQueue = snapshot.manual.map(QueueItem.init(bridge:))
        queueContext = snapshot.context.map(QueuePlaybackContext.init(bridge:))
        if snapshot.revision > revision {
            revision = snapshot.revision
            showLatestUpcoming()
        }
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

    /// Read `[offset, offset + limit)` of the context's upcoming tail through
    /// the store's one upcoming read, opening it on first use. A no-op when a
    /// window already read covers the range. Past `maximumUpcomingWindows`,
    /// the window farthest from this one is dropped from the read. Errors are
    /// logged because this is background prefetch with no separate error UI.
    @MainActor
    public func loadUpcomingRange(offset: Int, limit: Int, queue: Queue) async {
        guard let context = queueContext else {
            return
        }
        let end = min(offset + limit, context.upcomingTotal)
        guard offset < end else {
            return
        }
        let range = offset..<end
        if upcomingWindows.contains(where: {
            $0.lowerBound <= range.lowerBound
                && range.upperBound <= $0.upperBound
        }) {
            return
        }
        while upcomingWindows.count >= maximumUpcomingWindows {
            let midpoint = range.lowerBound + range.count / 2
            guard
                let farthest = upcomingWindows.indices.max(by: {
                    distance(from: upcomingWindows[$0], to: midpoint)
                        < distance(from: upcomingWindows[$1], to: midpoint)
                })
            else { break }
            upcomingWindows.remove(at: farthest)
        }
        upcomingWindows.append(range)
        let query = openUpcomingQuery(queue: queue)
        do {
            try query.setWindows(
                upcomingWindows.sorted { $0.lowerBound < $1.lowerBound }
                    .map {
                        BridgeLibraryPageWindow(
                            offset: UInt64($0.lowerBound),
                            limit: UInt64($0.count)
                        )
                    }
            )
        }
        catch {
            logger.warning(
                "upcoming range [\(offset), \(end)) was not requested: \(error.localizedDescription)"
            )
        }
    }

    @MainActor
    private func openUpcomingQuery(queue: Queue) -> QueueUpcomingQuery {
        if let upcomingQuery {
            return upcomingQuery
        }
        let query = queue.subscribeUpcoming()
        upcomingQuery = query
        upcomingDeliveries = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                do {
                    let snapshot = try await query.next()
                    guard let self else { return }
                    self.applyUpcoming(snapshot)
                }
                catch {
                    if !Task.isCancelled {
                        logger.warning(
                            "upcoming queue read failed: \(error.localizedDescription)"
                        )
                    }
                    return
                }
            }
        }
        return query
    }

    @MainActor
    private func applyUpcoming(_ snapshot: BridgeQueueUpcomingSnapshot) {
        latestUpcoming = snapshot
        guard snapshot.revision == revision else {
            return
        }
        showLatestUpcoming()
    }

    /// Show the newest upcoming value when it was sliced from the queue
    /// revision on screen, and nothing past the initial window otherwise:
    /// its offsets count from another queue's tail.
    private func showLatestUpcoming() {
        guard let latestUpcoming, latestUpcoming.revision == revision else {
            pagedUpcoming = [:]
            return
        }
        var items: [Int: QueueItem] = [:]
        for window in latestUpcoming.windows {
            for (i, entry) in window.entries.enumerated() {
                items[Int(window.window.offset) + i] = QueueItem(bridge: entry)
            }
        }
        pagedUpcoming = items
    }

    private func distance(from range: Range<Int>, to index: Int) -> Int {
        if index < range.lowerBound { return range.lowerBound - index }
        if index >= range.upperBound { return index - range.upperBound + 1 }
        return 0
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

extension BridgeSidePausePrompt {
    public func title() -> String {
        String(
            format: localizedCoreString(titleKey),
            sideLabel
        )
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

extension NowPlaying {
    fileprivate var sidePausePrompt: BridgeSidePausePrompt? {
        guard case .paused(_, let reason) = self else {
            return nil
        }
        return reason.sidePausePrompt
    }
}
