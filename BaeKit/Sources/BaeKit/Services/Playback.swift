import Foundation

/// Playback transport — commands the now-playing track stream.
/// Narrow subset of `AppHandle` covering play/pause, seek, volume,
/// repeat mode, the "play this release" trigger, and the pause-between-sides
/// preference. Views that drive transport take this instead of the full
/// `AppService`.
public final class Playback: Sendable, Observable {
    public let pause: @Sendable () -> Void
    public let resume: @Sendable () -> Void
    /// Stop a running side-pause countdown and stay paused at the boundary.
    /// Play still starts the next side.
    public let cancelSidePauseCountdown: @Sendable () -> Void
    public let nextTrack: @Sendable () -> Void
    public let previousTrack: @Sendable () -> Void
    public let seekByRatio: @Sendable (_ ratio: Double) -> Void
    /// Set the level where a volume drag ends: stored, and shown by every
    /// screen.
    public let setVolume: @Sendable (_ volume: Float) -> Void
    /// Move the output to the level a volume drag is at, storing nothing.
    public let setVolumeLive: @Sendable (_ volume: Float) -> Void
    public let setMuted: @Sendable (_ muted: Bool) -> Void
    /// Set the repeat mode to an absolute value. Every caller sends the mode it
    /// wants; a cycling button passes `mode.next` computed from what it renders.
    public let setRepeatMode: @Sendable (_ mode: BridgeRepeatMode) -> Void
    /// Play a release from a track, from its first track, or shuffled. What
    /// the play did shows on the app's error surface when it has a line: the
    /// release holds no tracks, or the track to start from is gone.
    public let playRelease:
        @Sendable (
            _ releaseId: String, _ startTrackId: String?, _ shuffle: Bool
        ) -> Void
    /// Play several releases as one context, concatenated in the given order.
    /// A single release behaves exactly like `playRelease`; core leaves out
    /// any release without tracks, and the line says how many.
    public let playReleases: @Sendable (_ releaseIds: [String]) -> Void
    /// Play the whole library in a freshly seeded shuffle. An empty library
    /// plays nothing, and the line says so.
    public let playLibraryShuffled: @Sendable () -> Void
    public let setPauseBetweenSides:
        @Sendable (_ enabled: Bool) async throws -> Void
    public let setSidePauseCountdown:
        @Sendable (_ countdown: BridgeSidePauseCountdown) async throws -> Void
    /// Whether the seek bar's leading label counts down the time remaining.
    /// A synced preference, so the bar writes it here rather than to a local
    /// store, and reads it back off the config mirror.
    public let setShowRemainingTime:
        @Sendable (_ enabled: Bool) async throws -> Void

    public init(
        pause: @escaping @Sendable () -> Void = {},
        resume: @escaping @Sendable () -> Void = {},
        cancelSidePauseCountdown: @escaping @Sendable () -> Void = {},
        nextTrack: @escaping @Sendable () -> Void = {},
        previousTrack: @escaping @Sendable () -> Void = {},
        seekByRatio: @escaping @Sendable (Double) -> Void = { _ in },
        setVolume: @escaping @Sendable (Float) -> Void = { _ in },
        setVolumeLive: @escaping @Sendable (Float) -> Void = { _ in },
        setMuted: @escaping @Sendable (Bool) -> Void = { _ in },
        setRepeatMode: @escaping @Sendable (BridgeRepeatMode) -> Void = {
            _ in
        },
        playRelease: @escaping @Sendable (String, String?, Bool) -> Void = {
            _,
            _,
            _ in
        },
        playReleases: @escaping @Sendable ([String]) -> Void = { _ in },
        playLibraryShuffled: @escaping @Sendable () -> Void = {},
        setPauseBetweenSides: @escaping @Sendable (Bool) async throws -> Void =
            {
                _ in
            },
        setSidePauseCountdown:
            @escaping @Sendable (BridgeSidePauseCountdown) async throws -> Void =
            {
                _ in
            },
        setShowRemainingTime: @escaping @Sendable (Bool) async throws -> Void =
            {
                _ in
            }
    ) {
        self.pause = pause
        self.resume = resume
        self.cancelSidePauseCountdown = cancelSidePauseCountdown
        self.nextTrack = nextTrack
        self.previousTrack = previousTrack
        self.seekByRatio = seekByRatio
        self.setVolume = setVolume
        self.setVolumeLive = setVolumeLive
        self.setMuted = setMuted
        self.setRepeatMode = setRepeatMode
        self.playRelease = playRelease
        self.playReleases = playReleases
        self.playLibraryShuffled = playLibraryShuffled
        self.setPauseBetweenSides = setPauseBetweenSides
        self.setSidePauseCountdown = setSidePauseCountdown
        self.setShowRemainingTime = setShowRemainingTime
    }

    /// Transport over `handle`. A play's outcome or failure that has a line
    /// goes to `show`.
    public convenience init(
        handle: any AppHandleProtocol,
        show: @escaping @MainActor @Sendable (DisplayError) -> Void
    ) {
        self.init(
            pause: { handle.pause() },
            resume: { handle.resume() },
            cancelSidePauseCountdown: { handle.cancelSidePauseCountdown() },
            nextTrack: { handle.nextTrack() },
            previousTrack: { handle.previousTrack() },
            seekByRatio: { handle.seekByRatio(ratio: $0) },
            setVolume: { handle.setVolume(volume: $0) },
            setVolumeLive: { handle.setVolumeLive(volume: $0) },
            setMuted: { handle.setMuted(muted: $0) },
            setRepeatMode: { handle.setRepeatMode(mode: $0) },
            playRelease: { releaseId, startTrackId, shuffle in
                Self.answer(show) {
                    try await handle.playRelease(
                        releaseId: releaseId,
                        startTrackId: startTrackId,
                        shuffle: shuffle
                    )
                }
            },
            playReleases: { releaseIds in
                Self.answer(show) {
                    try await handle.playReleases(releaseIds: releaseIds)
                }
            },
            playLibraryShuffled: {
                Self.answer(show) { try await handle.playLibraryShuffled() }
            },
            setPauseBetweenSides: {
                try await handle.setPauseBetweenSides(enabled: $0)
            },
            setSidePauseCountdown: {
                try await handle.setSidePauseCountdown(countdown: $0)
            },
            setShowRemainingTime: {
                try await handle.setShowRemainingTime(enabled: $0)
            }
        )
    }

    /// Run `play` and show what it did, or why it couldn't say: a play
    /// that played everything it named shows nothing.
    private static func answer(
        _ show: @escaping @MainActor @Sendable (DisplayError) -> Void,
        _ play: @escaping @Sendable () async throws -> BridgePlayOutcome
    ) {
        Task { @MainActor in
            do {
                if let shown = try await play().shown {
                    show(shown)
                }
            }
            catch {
                if let shown = DisplayError(error) {
                    show(shown)
                }
            }
        }
    }

    #if DEBUG
        // periphery:ignore
        public static func stub() -> Playback { Playback() }
    #endif
}

extension Playback {
    /// Send the absolute transport command for a play/pause press: pause
    /// while playing or loading (the transport shows the pause glyph through
    /// a load), resume while paused, nothing while stopped — there is no
    /// track to act on, and `resume` would also tear down an active import
    /// preview.
    public func playPause(for nowPlaying: NowPlaying) {
        switch nowPlaying {
        case .playing, .loading:
            pause()
        case .paused:
            resume()
        case .stopped:
            break
        }
    }
}

extension Playback {
    /// Answer the side/disc pause prompt. An unchecked pause-between-sides box
    /// turns the setting off; a checked one leaves it as it is. Play starts the
    /// next side now; Close stays paused and stops any countdown, so the next
    /// side waits for Play. Either happens even when the setting write fails —
    /// the failure is thrown after, for the caller to show.
    public func answerSidePausePrompt(keepPausing: Bool, play: Bool)
        async throws
    {
        var writeError: (any Error)?
        if !keepPausing {
            do {
                try await setPauseBetweenSides(false)
            }
            catch {
                writeError = error
            }
        }
        if play {
            resume()
        }
        else {
            cancelSidePauseCountdown()
        }
        if let writeError {
            throw writeError
        }
    }
}
