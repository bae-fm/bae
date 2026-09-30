import Combine
import SwiftUI
import os.log

private let logger = Logger.bae("PlaybackStore")
/// How many windows of the context's upcoming tail stay read at once: the one
/// around the visible rows and the two nearest it.
private let maximumUpcomingWindows = 3

/// Mirror of core's playback state, written by the playback and queue
/// subscriptions. Views read it and act through `appHandle`.
///
/// Not `@MainActor` as a whole because `MediaControlService.handleScrub` calls
/// `projectSeek` from a nonisolated callback; only the upcoming-read methods
/// are, since the `Task` they start captures `self`.
@Observable
public class PlaybackStore {
    public private(set) var nowPlaying: NowPlaying = .stopped
    private var dismissedSidePausePromptId: String?

    public var volume: Float = 1.0
    public var isMuted: Bool = false
    public var repeatMode: BridgeRepeatMode = .off
    /// The manual lane ("Up Next") — explicitly enqueued tracks, drained first.
    public var manualQueue: [QueueItem] = []
    /// The release being played from, or `nil`. `context.upcoming` holds only
    /// the first window; read any index through `upcomingItem(at:)`.
    public var queueContext: QueuePlaybackContext?
    /// Context-tail entries read past the initial window, keyed by their
    /// absolute index in the tail: the latest upcoming value whose revision
    /// matches `revision`, and empty while none does.
    public var pagedUpcoming: [Int: QueueItem] = [:]
    /// The queue revision the current `manualQueue`/`queueContext` were resolved
    /// from. Upcoming values sliced from any other revision are not shown.
    @ObservationIgnored
    public private(set) var revision: UInt64 = 0
    /// The live read of the upcoming tail, opened by the first
    /// `loadUpcomingRange` and reused after.
    @ObservationIgnored
    private var upcomingQuery: QueueUpcomingQuery?
    @ObservationIgnored
    private var upcomingDeliveries: Task<Void, Never>?
    /// The tail ranges the upcoming read covers, at most
    /// `maximumUpcomingWindows`; loading one past that drops the one farthest
    /// from it.
    @ObservationIgnored
    private var upcomingWindows: [Range<Int>] = []
    /// The newest upcoming value, held in case it arrives before the queue
    /// value of its revision.
    @ObservationIgnored
    private var latestUpcoming: BridgeQueueUpcomingSnapshot?

    /// Sent through Combine rather than `@Observable` because it changes at
    /// display rate; only the progress bar re-renders.
    @ObservationIgnored
    private let playbackPositionSubject = CurrentValueSubject<
        PlaybackPositionEvent, Never
    >(.reset)
    private var playbackPosition: PlaybackPositionState?

    /// The count of tracks each add puts in the queue; drives the queue
    /// button's "+N" badge.
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

    /// Enter loading for `trackId`, keeping the displayed track until
    /// `setLoadingTarget` brings the target's metadata; otherwise the
    /// now-playing bar, and on iOS the full-screen player, would close on every
    /// track change.
    public func beginLoading(trackId: String) {
        let previousTrackId = nowPlaying.track?.track.trackId
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

    /// The target's metadata arrived. It applies when already loading this
    /// track, or when playing or paused on it (core re-enters loading to buffer
    /// a seek); in any other state a newer load has moved on and it is dropped.
    public func setLoadingTarget(trackId: String, target: BridgeNowPlayingTrack)
    {
        let previous: BridgeNowPlayingTrack?
        switch nowPlaying {
        case .loading(trackId, _, let priorFallback):
            previous = priorFallback
        case .playing(let current) where current.track.trackId == trackId,
            .paused(let current, _) where current.track.trackId == trackId:
            previous = current
        default:
            // Logged so a stuck now-playing bar can be traced.
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

    public func pause(
        track: BridgeNowPlayingTrack,
        reason: BridgePlaybackPauseReason
    ) {
        preparePlaybackPosition(for: track)
        setNowPlaying(.paused(track, reason: reason))
    }

    public func play(track: BridgeNowPlayingTrack) {
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
        guard let currentTrackId = nowPlaying.track?.track.trackId,
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
            trackId: nowPlaying.track?.track.trackId
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

    private func preparePlaybackPosition(for track: BridgeNowPlayingTrack) {
        if track.track.trackId != nowPlaying.track?.track.trackId {
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

    /// The upcoming item at `index`, or `nil` when it is not loaded or past
    /// the end.
    public func upcomingItem(at index: Int) -> QueueItem? {
        guard let context = queueContext else {
            return nil
        }
        if index < context.upcoming.count {
            return context.upcoming[index]
        }
        return pagedUpcoming[index]
    }

    /// Read `[offset, offset + limit)` of the upcoming tail. Beyond
    /// `maximumUpcomingWindows`, the window farthest from this one is dropped.
    /// Errors are only logged: this is prefetch with no error UI.
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

    /// Show the newest upcoming value only when it matches the queue revision
    /// on screen; otherwise its offsets count from another queue's tail.
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
            format: localizedCoreString(
                bridgePauseBoundaryTitleKey(boundary: boundary)
            ),
            sideLabel
        )
    }

    /// The checkbox that keeps pausing at this kind of boundary.
    public func keepPausingLabel() -> String {
        localizedCoreString(
            bridgePauseBoundaryKeepPausingKey(boundary: boundary)
        )
    }

    /// The line counting down to the next side or disc at `now`, formatted
    /// against the current locale so the seconds take its plural form.
    public func countdownLine(
        _ countdown: BridgeSideCountdown,
        at now: Date
    ) -> String {
        String(
            format: localizedCoreString(
                bridgePauseBoundaryCountdownKey(boundary: boundary)
            ),
            locale: Locale.current,
            countdown.secondsLeft(at: now)
        )
    }
}

extension BridgeSideCountdown {
    /// When the next side starts.
    public var resumesAt: Date {
        Date(timeIntervalSince1970: TimeInterval(resumesAtMs) / 1000)
    }

    /// Whole seconds until the next side starts at `now`, rounded up so the
    /// line never reads 0 while the side has yet to start, and never below 0.
    public func secondsLeft(at now: Date) -> Int {
        let nowMs = Int64((now.timeIntervalSince1970 * 1000).rounded(.down))
        let remainingMs = max(0, resumesAtMs - nowMs)
        return Int((remainingMs + 999) / 1000)
    }
}

public enum NowPlaying {
    case stopped
    /// A track is being prepared. `target` is its metadata once core resolves
    /// it; until then `track` falls back to `previous`, what was on screen, so
    /// the now-playing UI stays up.
    case loading(
        trackId: String,
        target: BridgeNowPlayingTrack?,
        previous: BridgeNowPlayingTrack?
    )
    case playing(BridgeNowPlayingTrack)
    case paused(BridgeNowPlayingTrack, reason: BridgePlaybackPauseReason)

    public var isActive: Bool {
        if case .stopped = self {
            return false
        }
        return true
    }

    public var track: BridgeNowPlayingTrack? {
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
            return track.display.artistNames
        case .playing(let track):
            return track.display.artistNames
        case .loading(_, let target, let previous):
            if let target {
                return target.display.artistNames
            }
            if let previous {
                return previous.display.artistNames
            }
            return nil
        case .stopped:
            return nil
        }
    }

    /// The loading track's id, which can differ from the displayed track's.
    public var loadingTrackId: String? {
        switch self {
        case .loading(let trackId, _, _): trackId
        case .playing, .paused, .stopped: nil
        }
    }

    /// Loading counts as playing, so the transport keeps the pause glyph
    /// through a track change instead of flickering.
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
