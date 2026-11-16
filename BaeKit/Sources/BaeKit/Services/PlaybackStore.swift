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
    /// What playback is doing to which track, as core's latest value says.
    public private(set) var nowPlaying: BridgePlaybackValueState = .stopped
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
    /// The context's tracks after the playing one, with the order they read
    /// in and why a later read failed; `nil` until `showUpcoming` has opened
    /// them, and again once no context plays.
    private var upcoming: OpenUpcoming?
    /// Which `showUpcoming` call is the latest, so one a later call has
    /// replaced does not open a list.
    @ObservationIgnored
    private var upcomingOpening = 0

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

    /// Take the state core's latest value says.
    public func apply(_ state: BridgePlaybackValueState) {
        if nowPlaying.sidePausePrompt?.id != state.sidePausePrompt?.id {
            dismissedSidePausePromptId = nil
        }
        nowPlaying = state
    }

    /// Tracks went into Up Next: the queue button's badge shows how many.
    /// When no tracks went in, the store sends nothing.
    public func publishQueueItemsAdded(_ count: Int) {
        guard count > 0 else {
            return
        }
        queueItemsAddedSubject.send(count)
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
        upcoming?.list.positionCount ?? 0
    }

    /// The upcoming item at `index`, or `nil` when it is not read or past
    /// the end.
    @MainActor
    public func upcomingItem(at index: Int) -> BridgeQueueEntry? {
        upcoming?.list.row(at: index)
    }

    /// Names the open upcoming list, which changes when a new one replaces it
    /// and so restarts the rows' holds; `nil` while none is open.
    @MainActor
    public var upcomingEpoch: LoadEpoch? {
        upcoming?.list.loadEpoch
    }

    /// Hold the upcoming item at `index` for as long as the call runs, so the
    /// list reads the window around it.
    @MainActor
    public func holdUpcoming(at index: Int) async {
        await upcoming?.list.holdPage(containing: index)
    }

    /// Why the store could not read the upcoming list: its first read failed,
    /// or a later one did.
    @MainActor
    public var upcomingFailure: DisplayError? {
        upcoming?.readFailure ?? upcoming?.list.initialLoadError
    }

    /// Open the upcoming list in the lane's current order, shuffled or not,
    /// while a context plays, and wait for its count and first rows. The lane
    /// keeps showing the list in the other order until the new list's first
    /// read returns, so turning shuffle on or off does not empty the lane.
    @MainActor
    public func showUpcoming(queue: Queue) async {
        guard queueContext != nil else {
            return
        }
        if upcoming?.shuffled == shuffled {
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
                self?.upcoming?.readFailure = DisplayError(error)
            }
        )
        await list.loadInitial()
        guard upcomingOpening == opening, queueContext != nil else {
            list.cancel()
            return
        }
        upcoming?.list.cancel()
        upcoming = OpenUpcoming(list: list, shuffled: order)
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
        upcoming?.list.cancel()
        upcoming = nil
    }
}

/// The upcoming list `showUpcoming` opened: the context's tracks after the
/// playing one, read in windows with a count of their own.
private struct OpenUpcoming {
    let list: PaginatedList<BridgeQueueEntry>
    /// Whether `list` reads the tracks shuffled.
    let shuffled: Bool
    /// Why a later read of `list` failed. The list shows what it has, and
    /// the queue says it couldn't read the rest.
    var readFailure: DisplayError?
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

extension BridgeNowPlayingDisplay {
    /// The library's display of the track; `nil` once the library no longer
    /// holds it.
    public var library: BridgeTrackDisplay? {
        switch self {
        case .library(let display): display
        case .removed: nil
        }
    }

    /// The title the now-playing surfaces show: the track's, or the line
    /// saying the library no longer holds it.
    public var title: String {
        switch self {
        case .library(let display): display.title
        case .removed: localizedCoreString("core.playback.removed_track")
        }
    }
}

extension BridgeLoadingTrack {
    /// The track being loaded. Until core prepares it, its length is not
    /// known, so its `durationMs` is `nil`.
    public var nowPlayingTrack: BridgeNowPlayingTrack {
        switch self {
        case .unprepared(let trackId, let display):
            BridgeNowPlayingTrack(
                track: BridgePlayingTrack(trackId: trackId, durationMs: nil),
                display: display
            )
        case .prepared(let track): track
        }
    }
}

extension BridgePlaybackValueState {
    public var isActive: Bool {
        if case .stopped = self {
            return false
        }
        return true
    }

    /// The track the state names: the one loading, playing or paused.
    public var nowPlayingTrack: BridgeNowPlayingTrack? {
        switch self {
        case .playing(let track), .paused(let track, _): track
        case .loading(let loading, _): loading.nowPlayingTrack
        case .stopped: nil
        }
    }

    /// The id of the track the state names.
    public var trackId: String? { nowPlayingTrack?.track.trackId }

    /// What the library shows of the track the state names.
    public var display: BridgeNowPlayingDisplay? { nowPlayingTrack?.display }

    public var secondaryLine: String? {
        if case .paused(_, let reason) = self,
            let prompt = reason.sidePausePrompt
        {
            return prompt.title()
        }
        return display?.library?.artistLine.text
    }

    /// Whether a load waits on a download, for the arc around Play.
    public var waitsOnStorage: Bool {
        switch self {
        case .loading(_, let waitsOnStorage): waitsOnStorage
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

    fileprivate var sidePausePrompt: BridgeSidePausePrompt? {
        guard case .paused(_, let reason) = self else {
            return nil
        }
        return reason.sidePausePrompt
    }
}
