import Combine
import SwiftUI
import os.log

private let logger = Logger.bae("PlaybackStore")
/// How many windows of the context's upcoming tail stay read at once: the one
/// around the visible rows and the two nearest it.
private let maximumUpcomingWindows = 3

/// Mirror of core's playback state, written by the playback and queue
/// subscriptions. Views read it and act through `appHandle`. Where the
/// playing track is moves too often for it: the native views drawing that
/// read `trackPositions` each display frame.
///
/// Not `@MainActor` as a whole; only the upcoming-read methods are, since
/// the read they open hands its values to `self` on the main actor.
@Observable
public class PlaybackStore {
    public private(set) var nowPlaying: NowPlaying = .stopped
    private var dismissedSidePausePromptId: String?

    /// The volume, mute and repeat mode; `nil` until core's first playback
    /// value says them.
    public var settings: PlaybackSettings?
    /// The manual lane ("Up next") — explicitly enqueued tracks, drained first.
    public var manualQueue: [BridgeQueueEntry] = []
    /// Up Next's version as core's queue value carries it: how many writes
    /// have changed the lane, which a held reorder waits for.
    public private(set) var manualVersion: UInt64 = 0
    /// What's playing from, or `nil`. Its tracks are read through the upcoming
    /// read: `upcomingTotal` and `upcomingItem(at:)`.
    public var queueContext: QueuePlaybackContext?
    /// Whether shuffle is on, which holds with nothing playing: the next play
    /// starts shuffled.
    public private(set) var shuffled = false
    /// The length of the context's not-yet-played tail, from the upcoming
    /// read; 0 until the read answers.
    public private(set) var upcomingTotal = 0
    /// The tail entries the upcoming read last delivered, keyed by their index
    /// in the tail. Indices it did not read are absent.
    public private(set) var upcomingRows: [Int: BridgeQueueEntry] = [:]
    /// The context lane's version as core's upcoming read carries it: how
    /// many writes have changed its tracks after the playing one. A change
    /// restarts the lane's row loads, and a held reorder waits for it; a
    /// scroll leaves it.
    public private(set) var contextVersion: UInt64 = 0
    /// The live read of the upcoming tail, opened by `showUpcoming` and kept
    /// while a context plays.
    @ObservationIgnored
    private var upcomingQuery: QueueUpcomingQuery?
    @ObservationIgnored
    private var upcomingDeliveries:
        ScreenSubscription<PulledValues<BridgeQueueUpcomingSnapshot>>?
    /// The tail ranges the upcoming read covers, at most
    /// `maximumUpcomingWindows`; loading one past that drops the one farthest
    /// from it.
    @ObservationIgnored
    private var upcomingWindows: [Range<Int>] = []
    /// Why the upcoming read stopped, until `retryUpcoming` opens it again.
    /// Unread rows stay placeholders while it is set.
    public private(set) var upcomingFailure: DisplayError?

    /// Where the playing track is, under its track id, for the native views
    /// that read it each display frame.
    public let trackPositions: LatestStream<BridgeTrackPosition>

    /// The parts of the playing track whose bytes this device holds, under
    /// its track id, for the seek bar that reads them each display frame.
    public let trackBuffered: LatestStream<BridgeTrackBuffered>

    /// Whether the playing track's load waits on storage, under its track
    /// id, for the arc around Play that reads it each display frame.
    public let trackStorageWaits: LatestStream<Bool>

    /// The count of tracks each add puts in the queue; drives the queue
    /// button's "+N" badge.
    @ObservationIgnored
    private let queueItemsAddedSubject = PassthroughSubject<Int, Never>()

    public var queueItemsAddedPublisher: AnyPublisher<Int, Never> {
        queueItemsAddedSubject.eraseToAnyPublisher()
    }

    /// A store whose positions, held parts and storage waits the
    /// subscription fills, or, for a shot, fixed ones.
    public init(
        trackPositions: LatestStream<BridgeTrackPosition> = .trackPositions(),
        trackBuffered: LatestStream<BridgeTrackBuffered> = .trackBuffered(),
        trackStorageWaits: LatestStream<Bool> = .trackStorageWaits()
    ) {
        self.trackPositions = trackPositions
        self.trackBuffered = trackBuffered
        self.trackStorageWaits = trackStorageWaits
    }

    /// Where `trackId` is while it plays, for the native view drawing it;
    /// `nil` while no track is.
    public func trackPosition(forTrack trackId: String?)
        -> LatestSource<BridgeTrackPosition>?
    {
        trackId.map { trackPositions.source(for: $0) }
    }

    /// The parts of `trackId` whose bytes this device holds, for the seek
    /// bar drawing them; `nil` while no track plays.
    public func trackBuffered(forTrack trackId: String?)
        -> LatestSource<BridgeTrackBuffered>?
    {
        trackId.map { trackBuffered.source(for: $0) }
    }

    /// Whether the load of `trackId` waits on storage, for the arc around
    /// Play; `nil` while no track loads.
    public func trackStorageWait(forTrack trackId: String?)
        -> LatestSource<Bool>?
    {
        trackId.map { trackStorageWaits.source(for: $0) }
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
    }

    /// Enter loading for `trackId`, keeping the displayed track until
    /// `setLoadingTarget` brings the target's metadata; otherwise the
    /// now-playing bar would close on every track change.
    public func beginLoading(trackId: String) {
        setNowPlaying(
            .loading(
                .unprepared(trackId: trackId),
                previous: nowPlaying.track
            )
        )
    }

    /// The target's metadata arrived. It applies when already loading this
    /// track, or when playing or paused on it (core re-enters loading to buffer
    /// a seek); in any other state a newer load has moved on and it is dropped.
    public func setLoadingTarget(_ target: BridgeNowPlayingTrack) {
        let trackId = target.track.trackId
        let previous: BridgeNowPlayingTrack?
        switch nowPlaying {
        case .loading(let loading, let priorFallback)
        where loading.trackId == trackId:
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
            .loading(.prepared(track: target), previous: previous)
        )
    }

    public func pause(
        track: BridgeNowPlayingTrack,
        reason: BridgePlaybackPauseReason
    ) {
        setNowPlaying(.paused(track, reason: reason))
    }

    public func play(track: BridgeNowPlayingTrack) {
        setNowPlaying(.playing(track))
    }

    func publishQueueItemsAdded(_ count: Int) {
        queueItemsAddedSubject.send(count)
    }

    private func setNowPlaying(_ next: NowPlaying) {
        if nowPlaying.sidePausePrompt?.id != next.sidePausePrompt?.id {
            dismissedSidePausePromptId = nil
        }
        nowPlaying = next
    }
}

extension PlaybackStore {
    /// Take core's queue value. A value without a context ends the upcoming
    /// read: there is no tail to read.
    @MainActor
    public func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        manualQueue = snapshot.manual
        manualVersion = snapshot.manualVersion
        queueContext = snapshot.context.map(QueuePlaybackContext.init(bridge:))
        shuffled = snapshot.shuffled
        if queueContext == nil {
            closeUpcomingQuery()
            upcomingTotal = 0
            upcomingRows = [:]
        }
    }

    /// The upcoming item at `index`, or `nil` when it is not read or past
    /// the end.
    public func upcomingItem(at index: Int) -> BridgeQueueEntry? {
        upcomingRows[index]
    }

    /// Whether a read upcoming row is the entry `entryId`.
    public func upcomingHolds(entryId: String) -> Bool {
        upcomingRows.values.contains { $0.id == entryId }
    }

    /// Open the upcoming read while a context plays, so its length is known
    /// before any of its rows is asked for.
    @MainActor
    public func showUpcoming(queue: Queue) {
        guard queueContext != nil else {
            return
        }
        _ = openUpcomingQuery(queue: queue)
    }

    /// Read `[offset, offset + limit)` of the upcoming tail. Beyond
    /// `maximumUpcomingWindows`, the window farthest from this one is dropped.
    /// A failed request or read stops the read and sets `upcomingFailure`,
    /// which the queue shows with Retry.
    @MainActor
    public func loadUpcomingRange(offset: Int, limit: Int, queue: Queue) async {
        guard queueContext != nil else {
            return
        }
        let end = min(offset + limit, upcomingTotal)
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
                "upcoming range [\(offset), \(end)) was not requested: \(String(reflecting: error))"
            )
            // A request to a read a failure already stopped fails because of
            // it, so the failure shown stays the first one.
            if upcomingFailure == nil {
                upcomingFailure = DisplayError(error)
            }
        }
    }

    /// Open the upcoming read again over the windows it covered, after a
    /// failure stopped it.
    @MainActor
    public func retryUpcoming(queue: Queue) async {
        upcomingFailure = nil
        let covered = upcomingWindows
        closeUpcomingQuery()
        showUpcoming(queue: queue)
        for window in covered {
            await loadUpcomingRange(
                offset: window.lowerBound,
                limit: window.count,
                queue: queue
            )
        }
    }

    /// Stop the upcoming read and forget the windows it covered.
    @MainActor
    private func closeUpcomingQuery() {
        upcomingDeliveries?.stop()
        upcomingDeliveries = nil
        upcomingWindows = []
        upcomingQuery = nil
    }

    @MainActor
    private func openUpcomingQuery(queue: Queue) -> QueueUpcomingQuery {
        if let upcomingQuery {
            return upcomingQuery
        }
        let query = queue.subscribeUpcoming()
        upcomingQuery = query
        upcomingDeliveries = ScreenSubscription(
            query.values,
            onValue: { [weak self] snapshot in self?.applyUpcoming(snapshot) },
            onError: { [weak self] error in
                logger.warning(
                    "upcoming queue read failed: \(String(reflecting: error))"
                )
                self?.upcomingFailure = DisplayError(error)
                self?.upcomingDeliveries?.stop()
            }
        )
        return query
    }

    /// Show what the upcoming read delivered: the tail's length and the rows
    /// of every window, read together, so the two always agree.
    @MainActor
    private func applyUpcoming(_ snapshot: BridgeQueueUpcomingSnapshot) {
        var rows: [Int: BridgeQueueEntry] = [:]
        for window in snapshot.windows {
            for (i, entry) in window.rows.enumerated() {
                rows[Int(window.window.offset) + i] = entry
            }
        }
        upcomingTotal = Int(snapshot.upcomingTotal)
        upcomingRows = rows
        contextVersion = snapshot.contextVersion
    }

    private func distance(from range: Range<Int>, to index: Int) -> Int {
        if index < range.lowerBound { return range.lowerBound - index }
        if index >= range.upperBound { return index - range.upperBound + 1 }
        return 0
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

extension BridgeLoadingTrack {
    /// The id of the track being loaded.
    public var trackId: String {
        switch self {
        case .unprepared(let trackId): trackId
        case .prepared(let track): track.track.trackId
        }
    }

    /// The loading track once core has prepared it.
    public var prepared: BridgeNowPlayingTrack? {
        switch self {
        case .unprepared: nil
        case .prepared(let track): track
        }
    }
}

public enum NowPlaying {
    case stopped
    /// A track is being loaded. Until core has prepared it, `track` falls back
    /// to `previous`, what was on screen, so the now-playing UI stays up.
    case loading(BridgeLoadingTrack, previous: BridgeNowPlayingTrack?)
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
        case .loading(let loading, let previous): loading.prepared ?? previous
        case .stopped: nil
        }
    }

    /// The track core says is current: the one playing or paused, or the
    /// loading one once core has prepared it. `nil` while the next track is
    /// only an id, when `track` still shows the one before it so the bar does
    /// not close on every track change; what marks or opens the current track
    /// reads this, not `track`.
    public var current: BridgeNowPlayingTrack? {
        switch self {
        case .playing(let t), .paused(let t, _): t
        case .loading(let loading, _): loading.prepared
        case .stopped: nil
        }
    }

    /// The id of the current track, the loading one included before core has
    /// prepared it.
    public var currentTrackId: String? {
        switch self {
        case .playing(let t), .paused(let t, _): t.track.trackId
        case .loading(let loading, _): loading.trackId
        case .stopped: nil
        }
    }

    public var secondaryLine: String? {
        switch self {
        case .paused(let track, let reason):
            if let prompt = reason.sidePausePrompt {
                return prompt.title()
            }
            return track.display.artistLine.text
        case .playing(let track):
            return track.display.artistLine.text
        case .loading:
            return track?.display.artistLine.text
        case .stopped:
            return nil
        }
    }

    /// The loading track's id, which can differ from the displayed track's.
    public var loadingTrackId: String? {
        switch self {
        case .loading(let loading, _): loading.trackId
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
