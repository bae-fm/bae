import Combine
import SwiftUI
import os.log

private let logger = Logger.bae("PlaybackStore")

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
    /// Why playback last failed, which the failure alert shows; `nil` once
    /// playback plays again or the person dismisses the alert.
    public internal(set) var failure: BridgePlaybackErrorReason?
    /// The manual lane ("Up next") — explicitly enqueued tracks, drained first.
    public var manualQueue: [BridgeQueueEntry] = []
    /// What's playing from, or `nil`. Its tracks are the `upcoming` list.
    public var queueContext: QueuePlaybackContext?
    /// Whether shuffle is on, which holds with nothing playing: the next play
    /// starts shuffled.
    public private(set) var shuffled = false
    /// The context's tracks after the playing one, as a list read in windows
    /// with a count of its own; `nil` until `showUpcoming` has opened it, and
    /// again once no context plays.
    private var upcoming: PaginatedList<BridgeQueueEntry>?
    /// Which order `upcoming` reads in.
    @ObservationIgnored
    private var upcomingShuffled = false
    /// Which `showUpcoming` call is the latest, so one a later call has
    /// replaced does not open a list.
    @ObservationIgnored
    private var upcomingOpening = 0
    /// A later read of `upcoming` failed. The list shows what it has, and the
    /// queue says it couldn't read the rest.
    private var upcomingReadFailure: DisplayError?

    /// Where the playing track is, under its track id, for the native views
    /// that read it each display frame.
    public let trackPositions: LatestStream<BridgeTrackPosition>

    /// The parts of the playing track whose bytes this device holds, under
    /// its track id, for the seek bar that reads them each display frame.
    public let trackBuffered: LatestStream<BridgeTrackBuffered>

    /// The count of tracks each add puts in the queue; drives the queue
    /// button's "+N" badge.
    @ObservationIgnored
    private let queueItemsAddedSubject = PassthroughSubject<Int, Never>()

    public var queueItemsAddedPublisher: AnyPublisher<Int, Never> {
        queueItemsAddedSubject.eraseToAnyPublisher()
    }

    /// A store whose positions and held parts the subscription fills, or,
    /// for a shot, fixed ones.
    public init(
        trackPositions: LatestStream<BridgeTrackPosition> = .trackPositions(),
        trackBuffered: LatestStream<BridgeTrackBuffered> = .trackBuffered()
    ) {
        self.trackPositions = trackPositions
        self.trackBuffered = trackBuffered
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
    /// now-playing bar would close on every track change. `waitsOnStorage`
    /// while the load waits on a download.
    public func beginLoading(trackId: String, waitsOnStorage: Bool) {
        setNowPlaying(
            .loading(
                .unprepared(trackId: trackId),
                previous: nowPlaying.track,
                waitsOnStorage: waitsOnStorage
            )
        )
    }

    /// The target's metadata arrived. It applies when already loading this
    /// track, or when playing or paused on it (core re-enters loading to buffer
    /// a seek); in any other state a newer load has moved on and it is dropped.
    /// `waitsOnStorage` while the load waits on a download.
    public func setLoadingTarget(
        _ target: BridgeNowPlayingTrack,
        waitsOnStorage: Bool
    ) {
        let trackId = target.track.trackId
        let previous: BridgeNowPlayingTrack?
        switch nowPlaying {
        case .loading(let loading, let priorFallback, _)
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
            .loading(
                .prepared(track: target),
                previous: previous,
                waitsOnStorage: waitsOnStorage
            )
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

    /// Tracks went into Up Next: the queue button's badge shows how many.
    /// Nothing is shown when none went in.
    public func publishQueueItemsAdded(_ count: Int) {
        guard count > 0 else {
            return
        }
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
    /// list: there is no tail to read.
    @MainActor
    public func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        manualQueue = snapshot.manual
        queueContext = snapshot.context.map(QueuePlaybackContext.init(bridge:))
        shuffled = snapshot.shuffled
        if queueContext == nil {
            closeUpcoming()
        }
    }

    /// The length of the context's tail, from the upcoming list's count; 0
    /// until the list is open and has answered.
    @MainActor
    public var upcomingTotal: Int {
        upcoming?.totalCount ?? 0
    }

    /// The upcoming item at `index`, or `nil` when it is not read or past
    /// the end.
    @MainActor
    public func upcomingItem(at index: Int) -> BridgeQueueEntry? {
        upcoming?.row(at: index)
    }

    /// Names the open upcoming list, which changes when a new one replaces it
    /// and so restarts the rows' holds; `nil` while none is open.
    @MainActor
    public var upcomingEpoch: LoadEpoch? {
        upcoming?.loadEpoch
    }

    /// Hold the upcoming item at `index` for as long as the call runs, so the
    /// list reads the window around it.
    @MainActor
    public func holdUpcoming(at index: Int) async {
        await upcoming?.holdPage(containing: index)
    }

    /// Why the upcoming list could not be read, when it could not: its first
    /// read failing, or a later one.
    @MainActor
    public var upcomingFailure: DisplayError? {
        upcomingReadFailure ?? upcoming?.initialLoadError
    }

    /// Open the upcoming list in the order the lane reads in now, while a
    /// context plays, and wait for its count and first rows. A list open in
    /// the other order stays shown until the new one has answered, so a
    /// shuffle flip does not empty the lane.
    @MainActor
    public func showUpcoming(queue: Queue) async {
        guard queueContext != nil else {
            return
        }
        if upcoming != nil, upcomingShuffled == shuffled {
            return
        }
        upcomingOpening += 1
        let opening = upcomingOpening
        let order = shuffled
        let list = PaginatedList(
            queries: queue.upcomingQueries(order),
            onError: { [weak self] error in
                logger.warning(
                    "upcoming queue read failed: \(String(reflecting: error))"
                )
                self?.upcomingReadFailure = DisplayError(error)
            }
        )
        await list.loadInitial()
        guard upcomingOpening == opening, queueContext != nil else {
            list.cancel()
            return
        }
        upcoming?.cancel()
        upcoming = list
        upcomingShuffled = order
    }

    /// Open the upcoming list again after a failure.
    @MainActor
    public func retryUpcoming(queue: Queue) async {
        closeUpcoming()
        await showUpcoming(queue: queue)
    }

    /// Stop the upcoming list and forget it.
    @MainActor
    private func closeUpcoming() {
        upcomingOpening += 1
        upcoming?.cancel()
        upcoming = nil
        upcomingReadFailure = nil
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
    /// `waitsOnStorage` while the load waits on a download.
    case loading(
        BridgeLoadingTrack,
        previous: BridgeNowPlayingTrack?,
        waitsOnStorage: Bool
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
        case .loading(let loading, let previous, _):
            loading.prepared ?? previous
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
        case .loading(let loading, _, _): loading.prepared
        case .stopped: nil
        }
    }

    /// The id of the current track, the loading one included before core has
    /// prepared it.
    public var currentTrackId: String? {
        switch self {
        case .playing(let t), .paused(let t, _): t.track.trackId
        case .loading(let loading, _, _): loading.trackId
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

    /// Whether a load waits on a download, for the arc around Play.
    public var waitsOnStorage: Bool {
        switch self {
        case .loading(_, _, let waitsOnStorage): waitsOnStorage
        case .playing, .paused, .stopped: false
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
