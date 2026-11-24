import Foundation
import MediaPlayer
import os.log

#if canImport(AppKit)
    import AppKit
#elseif canImport(UIKit)
    import UIKit
#endif

private let logger = Logger.bae("MediaControlService")

/// What `setNowPlaying` writes into `MPNowPlayingInfoCenter` for a library
/// track: the track, with what the library shows of it and its length
/// (`nil` while the length is unknown or core has not prepared the track),
/// and `playbackRate`, 1.0 while playing and 0.0 otherwise. The type is
/// internal: only `libraryMetadata(for:)`, which
/// `updateNowPlaying(state:images:)` calls, creates one.
struct NowPlayingMetadata: Equatable {
    let track: BridgeNowPlayingTrack
    let playbackRate: Double

    var trackId: String { track.track.trackId }
}

/// What the Now Playing surface shows: nothing, a library track, or (on
/// macOS) an import preview's clip. A position is written onto it only when
/// it is of that track or clip.
enum NowPlayingSubject {
    case nothing
    case library(trackId: String)
    #if os(macOS)
        case preview(BridgePreviewTarget)
    #endif
}

/// Transport intents the remote command center forwards to, each routed
/// through the preview-or-library dispatch. Each returns the status the command
/// handler reports back to the system.
struct TransportActions {
    let play: () -> MPRemoteCommandHandlerStatus
    let pause: () -> MPRemoteCommandHandlerStatus
    let toggle: () -> MPRemoteCommandHandlerStatus
    let next: () -> MPRemoteCommandHandlerStatus
    let previous: () -> MPRemoteCommandHandlerStatus
    let seek: (TimeInterval) -> MPRemoteCommandHandlerStatus
}

/// Bridges playback state to the macOS Now Playing surface (the Control Center
/// widget + media keys) and the remote command center. The shared core owns the
/// metadata, position, artwork, and scrub math; the preview session lives in
/// an os-gated extension.
public final class MediaControlService: @unchecked Sendable {
    var currentDurationMs: UInt64?
    /// Mirrors `changePlaybackPositionCommand.isEnabled` so the scrubber is
    /// toggled (and its disable logged) only on a real change — the duration
    /// path runs on every Now Playing write, so writing each time would spam.
    private var scrubbingEnabled = false
    /// The cover whose artwork Now Playing's info holds, or `nil` while it
    /// holds none. The pictures themselves are the image store's.
    private var shownArtwork: BridgeImageRef?
    private var artworkTask: Task<Void, Never>?
    /// What Now Playing shows.
    var shown: NowPlayingSubject = .nothing
    /// The latest timeline core sent. It arrives on its own subscription,
    /// so it can come before the values that put its track or clip on Now
    /// Playing; each values update shows the latest timeline again.
    private var latestTimeline: BridgePlaybackTimeline?
    /// What `setNowPlaying` last wrote for a library track, so the same
    /// metadata is not written again; `nil` while a library track is not shown.
    var writtenLibrary: NowPlayingMetadata?
    /// The playback rate the state gives: 1.0 playing, 0.0 otherwise.
    var rate: Double = 0.0
    /// Whether the library line core last told moves. Through a pregap it
    /// stands at the track's start while the track plays, and so does Now
    /// Playing's elapsed time.
    var lineMoves = true
    /// The elapsed time Now Playing was last given; `nil` while it shows
    /// nothing, or a track or clip no position has been written for yet.
    var anchor: ElapsedAnchor?
    let now: @Sendable () -> ContinuousClock.Instant

    /// Whether a preview clip currently owns Now Playing. Only the macOS
    /// preview session shows one, so shared bodies read it without a gate.
    var isShowingPreview: Bool {
        #if os(macOS)
            if case .preview = shown {
                return true
            }
        #endif
        return false
    }

    #if os(macOS)
        var activeSession: ActiveMediaSession?
    #endif

    /// `now` is the clock the elapsed time is measured against, which the
    /// system moves Now Playing's elapsed time forward by.
    public init(
        now: @escaping @Sendable () -> ContinuousClock.Instant = {
            ContinuousClock.now
        }
    ) {
        self.now = now
    }

    // MARK: - Playback values

    /// Translate core's resolved media-control presentation onto the platform's
    /// single Now Playing surface: the preview clip it names, or the library
    /// playback in `values.state`. A track or clip no elapsed time has been
    /// written for yet takes the latest timeline's line, which can arrive
    /// before the values that show it. One already moving keeps its line,
    /// rebased at a new rate, until core's timeline tells the next.
    public func applyValues(
        _ values: BridgePlaybackValues,
        images: ImageStore
    ) {
        switch values.mediaControl {
        case .preview(let target, let durationMs, let isPlaying):
            #if os(macOS)
                updatePreviewNowPlaying(
                    target: target,
                    durationMs: durationMs,
                    isPlaying: isPlaying
                )
            #endif
        case .library:
            updateNowPlaying(state: values.state, images: images)
        }
        if anchor == nil {
            showLatestTimeline()
        }
    }

    /// Write core's latest line onto Now Playing: the elapsed time of the
    /// track or clip it shows. Core tells a line only as playback leaves the
    /// last one, so each is written.
    public func applyTimeline(_ timeline: BridgePlaybackTimeline) {
        latestTimeline = timeline
        showLatestTimeline()
    }

    private func showLatestTimeline() {
        guard let latestTimeline else {
            return
        }
        switch shown {
        case .nothing:
            return
        case .library(let trackId):
            guard let line = latestTimeline.library else {
                return
            }
            guard line.trackId == trackId else {
                logger.debug(
                    "Not showing the position of \(line.trackId); Now Playing shows \(trackId)"
                )
                return
            }
            updatePosition(
                positionMs: line.positionMs,
                durationMs: line.durationMs,
                moving: line.moving
            )
        #if os(macOS)
            case .preview(let target):
                guard let line = latestTimeline.preview else {
                    return
                }
                guard line.target == target else {
                    logger.debug(
                        "Not showing the position of preview \(URL(filePathBytes: line.target.path, isDirectory: false).path); Now Playing shows \(URL(filePathBytes: target.path, isDirectory: false).path)"
                    )
                    return
                }
                updatePreviewPosition(positionMs: line.positionMs)
        #endif
        }
    }

    // MARK: - Library Now Playing

    /// Push the Now Playing info for a library playback state: clear Now
    /// Playing for a stopped state, and write the state's track to it for
    /// every other state.
    public func updateNowPlaying(
        state: BridgePlaybackValueState,
        images: ImageStore
    ) {
        guard let metadata = Self.libraryMetadata(for: state) else {
            clearNowPlaying()
            return
        }
        setNowPlaying(
            metadata,
            images: images,
            on: MPNowPlayingInfoCenter.default()
        )
    }

    /// The Now Playing fields for a state, or `nil` when it is stopped. The
    /// `playing` rate is 1.0; `paused` and `loading` are 0.0 (no audio is
    /// flowing yet for the latter).
    private static func libraryMetadata(
        for state: BridgePlaybackValueState
    ) -> NowPlayingMetadata? {
        switch state {
        case .playing(let track):
            return NowPlayingMetadata(track: track, playbackRate: 1.0)
        case .paused(let track, _):
            return NowPlayingMetadata(track: track, playbackRate: 0.0)
        case .loading(let loading, _):
            return NowPlayingMetadata(
                track: loading.nowPlayingTrack,
                playbackRate: 0.0
            )
        case .stopped:
            return nil
        }
    }

    /// Push the Now Playing info for a library track, unless it is what was
    /// written last. Duration is tracked through `trackDuration`, which enables
    /// the scrubber only while a track duration is known. The same track at a
    /// new rate keeps its elapsed time moving from where it is now; another
    /// track waits for its first position.
    func setNowPlaying(
        _ metadata: NowPlayingMetadata,
        images: ImageStore,
        on infoCenter: MPNowPlayingInfoCenter
    ) {
        guard metadata != writtenLibrary else {
            return
        }
        var info = infoCenter.nowPlayingInfo ?? [:]
        if writtenLibrary?.trackId == metadata.trackId {
            anchor = anchor?
                .rebased(
                    rate: lineMoves ? metadata.playbackRate : 0,
                    at: now()
                )
        }
        else {
            anchor = nil
            lineMoves = true
            info.removeValue(
                forKey: MPNowPlayingInfoPropertyElapsedPlaybackTime
            )
        }
        writtenLibrary = metadata
        rate = metadata.playbackRate
        shown = .library(trackId: metadata.trackId)
        let library = metadata.track.display.library
        info[MPMediaItemPropertyTitle] = metadata.track.display.title
        info[MPMediaItemPropertyArtist] = library?.artistLine.text
        info[MPMediaItemPropertyAlbumTitle] = library?.albumTitle
        trackDuration(metadata.track.track.durationMs, into: &info)
        info[MPNowPlayingInfoPropertyPlaybackRate] = metadata.playbackRate
        applyArtwork(
            image: library?.coverImage,
            images: images,
            into: &info
        )
        publish(info, on: infoCenter)
    }

    /// Set Now Playing's elapsed time to `positionMs`, from which the system
    /// moves it forward at the shown rate while the line `moving`.
    public func updatePosition(
        positionMs: UInt64,
        durationMs: UInt64?,
        moving: Bool
    ) {
        let infoCenter = MPNowPlayingInfoCenter.default()
        guard var info = infoCenter.nowPlayingInfo else {
            return
        }
        lineMoves = moving
        anchor = ElapsedAnchor(
            positionMs: positionMs,
            rate: moving ? rate : 0,
            at: now()
        )
        // Re-track here too: core may only learn the duration once decoding
        // starts, so it can arrive on progress rather than the Playing payload.
        trackDuration(durationMs, into: &info)
        publish(info, on: infoCenter)
    }

    /// Write `info` with the elapsed time where the anchor puts it now: the
    /// system measures the elapsed time from each write, so every write carries
    /// it.
    func publish(_ info: [String: Any], on infoCenter: MPNowPlayingInfoCenter) {
        var info = info
        if let anchor {
            info[MPNowPlayingInfoPropertyElapsedPlaybackTime] =
                max(0, anchor.positionMs(at: now())) / 1000.0
            // The system moves the elapsed time on at this rate.
            info[MPNowPlayingInfoPropertyPlaybackRate] = anchor.rate
        }
        infoCenter.nowPlayingInfo = info
        wroteNowPlaying()
    }

    /// Track the live duration so the scrubber is enabled only while a track
    /// duration is known. Core reports no duration (nil) for a track whose
    /// length is unknown, mirroring Android's `C.TIME_UNSET`.
    /// With a known duration the scrubber is enabled and a seek maps onto it;
    /// with an unknown one we drop the duration and disable the scrub command,
    /// so a drag can't reach a handler that would silently no-op against a
    /// missing duration.
    func trackDuration(
        _ durationMs: UInt64?,
        into info: inout [String: Any]
    ) {
        currentDurationMs = durationMs
        if let currentDurationMs {
            info[MPMediaItemPropertyPlaybackDuration] =
                Double(currentDurationMs) / 1000.0
        }
        else {
            info.removeValue(forKey: MPMediaItemPropertyPlaybackDuration)
        }
        setScrubbingEnabled(currentDurationMs != nil)
    }

    /// Single owner of the scrub command's enabled state, so the
    /// duration-tracking path and teardown can't drift. Acts only on a real
    /// change and logs the disable — a known→unknown duration is a legitimate
    /// skip (the seek would have no timeline to map onto) worth surfacing once.
    private func setScrubbingEnabled(_ enabled: Bool) {
        guard enabled != scrubbingEnabled else {
            return
        }
        scrubbingEnabled = enabled
        MPRemoteCommandCenter.shared().changePlaybackPositionCommand.isEnabled =
            enabled
        if !enabled {
            logger.debug("Lock-screen scrubber disabled: no known duration")
        }
    }

    /// Each queue delivery calls this; a command is written only when its
    /// availability changes.
    public func updateCommandAvailability(hasNext: Bool, hasPrevious: Bool) {
        let center = MPRemoteCommandCenter.shared()
        if center.nextTrackCommand.isEnabled != hasNext {
            center.nextTrackCommand.isEnabled = hasNext
        }
        if center.previousTrackCommand.isEnabled != hasPrevious {
            center.previousTrackCommand.isEnabled = hasPrevious
        }
    }

    /// Wipe Now Playing and disable every transport command. Called when
    /// playback stops or the library is torn down.
    public func clearNowPlaying() {
        shown = .nothing
        writtenLibrary = nil
        rate = 0.0
        anchor = nil
        currentDurationMs = nil
        clearArtworkLoad()
        MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
        wroteNowPlaying()
        // Nothing is playing, so disable the transport — leaving next/previous
        // enabled invites taps core ignores — and drop the scrubber (one owner).
        updateCommandAvailability(hasNext: false, hasPrevious: false)
        setScrubbingEnabled(false)
    }
}

extension MediaControlService {
    // MARK: - Scrubbing

    /// The seek ratio for a scrub to `positionTime`, or `nil` (logged) when no
    /// duration is tracked — the seek would have no timeline to map onto.
    func scrubRatio(for positionTime: TimeInterval) -> Double? {
        guard let durationMs = currentDurationMs, durationMs > 0 else {
            logger.error("Scrubber seek ignored: no current duration tracked")
            return nil
        }
        return positionTime / (Double(durationMs) / 1000.0)
    }

    /// Shared library-scrub math: map the drag onto the timeline, move Now
    /// Playing's elapsed time there at once, then dispatch the actual seek,
    /// whose line core's timeline tells once applied. `seekByRatio` is the
    /// platform's transport call.
    func handleScrub(
        positionTime: TimeInterval,
        seekByRatio: (Double) -> Void
    ) -> MPRemoteCommandHandlerStatus {
        guard let ratio = scrubRatio(for: positionTime) else {
            return .commandFailed
        }
        // A scrub lands on the track's own time, past any pregap.
        updatePosition(
            positionMs: UInt64(max(0, positionTime * 1000).rounded()),
            durationMs: currentDurationMs,
            moving: true
        )
        seekByRatio(ratio)
        return .success
    }

    /// Register the six transport commands against `center`. The command center
    /// is process-global; a library re-open builds a new service whose targets
    /// would stack on top of the prior instance's stale closures, so every
    /// command is cleared before re-adding.
    func registerTransportCommands(
        center: MPRemoteCommandCenter,
        actions: TransportActions
    ) {
        center.playCommand.removeTarget(nil)
        center.pauseCommand.removeTarget(nil)
        center.togglePlayPauseCommand.removeTarget(nil)
        center.nextTrackCommand.removeTarget(nil)
        center.previousTrackCommand.removeTarget(nil)
        center.changePlaybackPositionCommand.removeTarget(nil)

        center.playCommand.addTarget { _ in actions.play() }
        center.pauseCommand.addTarget { _ in actions.pause() }
        center.togglePlayPauseCommand.addTarget { _ in actions.toggle() }
        center.nextTrackCommand.addTarget { _ in actions.next() }
        center.previousTrackCommand.addTarget { _ in actions.previous() }
        center.changePlaybackPositionCommand.addTarget { event in
            guard
                let positionEvent = event
                    as? MPChangePlaybackPositionCommandEvent
            else {
                return .noActionableNowPlayingItem
            }
            return actions.seek(positionEvent.positionTime)
        }
    }

    // MARK: - Artwork

    /// Keep the artwork Now Playing holds when it is already the cover's,
    /// else clear the slot and ask the image store for the cover in the
    /// background, writing it into the Now Playing info when ready. The
    /// reference pins the content version, so replacing a release's cover
    /// reloads the artwork.
    private func applyArtwork(
        image: BridgeImageRef?,
        images: ImageStore,
        into info: inout [String: Any]
    ) {
        guard let image else {
            clearArtworkLoad()
            info.removeValue(forKey: MPMediaItemPropertyArtwork)
            return
        }
        if image == shownArtwork, info[MPMediaItemPropertyArtwork] != nil {
            return
        }
        info.removeValue(forKey: MPMediaItemPropertyArtwork)
        shownArtwork = nil
        artworkTask?.cancel()
        artworkTask = Task { [weak self] in
            await self?.loadArtwork(image: image, images: images)
        }
    }

    private func loadArtwork(image: BridgeImageRef, images: ImageStore) async {
        guard let decoded = await artwork(of: image, from: images) else {
            return
        }
        guard !Task.isCancelled else {
            return
        }
        let box = SendablePlatformImage(decoded)
        let artwork = MPMediaItemArtwork(boundsSize: decoded.size) { _ in
            box.value
        }
        await MainActor.run {
            guard !Task.isCancelled else {
                return
            }
            let infoCenter = MPNowPlayingInfoCenter.default()
            // Don't resurrect Now Playing: if it was cleared while the artwork
            // loaded (stop / library switch), drop the result rather than
            // writing an artwork-only entry.
            guard var info = infoCenter.nowPlayingInfo else {
                return
            }
            shownArtwork = image
            info[MPMediaItemPropertyArtwork] = artwork
            publish(info, on: infoCenter)
        }
    }

    /// The cover decoded by the image store at Now Playing's size, or `nil`
    /// when the library has none or the load failed (logged).
    private func artwork(
        of image: BridgeImageRef,
        from images: ImageStore
    ) async -> PlatformImage? {
        let scale = await artworkDisplayScale()
        do {
            guard
                let decoded = try await images.image(
                    .libraryImage(image),
                    pointSize: Self.artworkPointSize,
                    displayScale: scale
                )
            else {
                logger.debug("No Now Playing artwork for \(image.id)")
                return nil
            }
            return decoded
        }
        catch is CancellationError {
            return nil
        }
        catch {
            logger.warning(
                "Failed to load Now Playing artwork \(image.id): \(error)"
            )
            return nil
        }
    }

    /// The width, in points, of the cover Now Playing is given.
    private static let artworkPointSize: CGFloat = 600

    private func artworkDisplayScale() async -> CGFloat {
        await MainActor.run {
            #if os(macOS)
                NSScreen.main?.backingScaleFactor ?? 2.0
            #else
                UIScreen.main.scale
            #endif
        }
    }

    private func clearArtworkLoad() {
        artworkTask?.cancel()
        artworkTask = nil
        shownArtwork = nil
    }
}

/// `PlatformImage` (`NSImage`/`UIImage`) is documented thread-safe for reads
/// after construction, but isn't `Sendable`. Box it so the artwork request
/// closure can cross the concurrency boundary `MPMediaItemArtwork` expects.
private final class SendablePlatformImage: @unchecked Sendable {
    let value: PlatformImage
    init(_ value: PlatformImage) {
        self.value = value
    }
}

extension MediaControlService {
    #if DEBUG
        /// Posted after each write of the system's Now Playing info, for a
        /// test to wait on.
        public static let didWriteNowPlaying = Notification.Name(
            "MediaControlService.didWriteNowPlaying"
        )
    #endif

    func wroteNowPlaying() {
        #if DEBUG
            NotificationCenter.default.post(
                name: Self.didWriteNowPlaying,
                object: nil
            )
        #endif
    }
}
