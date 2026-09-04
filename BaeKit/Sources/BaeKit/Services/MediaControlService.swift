import Foundation
import MediaPlayer
import os.log

#if canImport(AppKit)
    import AppKit
#elseif canImport(UIKit)
    import UIKit
#endif

private let logger = Logger.bae("MediaControlService")

/// The Now Playing fields for a library track: what `setNowPlaying` writes into
/// `MPNowPlayingInfoCenter`, grouped so the call takes one metadata value rather
/// than several positional parameters. `playbackRate` (1.0 playing, 0.0 paused)
/// is the only field that differs between the playing and paused states.
/// Internal — construction happens only inside `updateNowPlaying(state:appHandle:)`
/// via `libraryMetadata(for:)`, both in this module.
struct NowPlayingMetadata {
    let trackTitle: String
    let artistNames: String
    let albumTitle: String
    let durationMs: UInt64
    let coverImage: BridgeImageRef?
    let playbackRate: Double
}

/// Transport intents the remote command center forwards to. macOS routes each
/// through its preview-or-library dispatch; iOS calls straight into `Playback`.
/// Each returns the status the command handler reports back to the system.
struct TransportActions {
    let play: () -> MPRemoteCommandHandlerStatus
    let pause: () -> MPRemoteCommandHandlerStatus
    let toggle: () -> MPRemoteCommandHandlerStatus
    let next: () -> MPRemoteCommandHandlerStatus
    let previous: () -> MPRemoteCommandHandlerStatus
    let seek: (TimeInterval) -> MPRemoteCommandHandlerStatus
}

/// Bridges playback state to the platform's Now Playing surface (the macOS
/// Control Center widget + media keys, the iOS lock screen + Control Center)
/// and the remote command center. The shared core owns the metadata, position,
/// artwork, and scrub math; the macOS preview session and the iOS audio session
/// live in os-gated extensions.
public final class MediaControlService: @unchecked Sendable {
    var currentDurationMs: UInt64?
    /// Mirrors `changePlaybackPositionCommand.isEnabled` so the scrubber is
    /// toggled (and its disable logged) only on a real change — the duration
    /// path runs on every progress tick, so writing each time would spam.
    private var scrubbingEnabled = false
    private var cachedArtworkImage: BridgeImageRef?
    private var cachedArtwork: MPMediaItemArtwork?
    private var artworkTask: Task<Void, Never>?
    /// Whether a preview clip currently owns Now Playing. Only macOS writes it
    /// (iOS has no import preview), so shared bodies read it without a gate.
    var isShowingPreview = false

    #if os(macOS)
        var activeSession: ActiveMediaSession?
    #endif

    #if os(iOS)
        var sessionActivated = false
        var observersRegistered = false
        /// Latched from the most recent Now Playing update so the interruption
        /// handler knows whether playback was active when a call/alarm began.
        var lastKnownIsPlaying = false
        /// Set when an interruption paused active playback, so `.ended` only
        /// auto-resumes playback we paused — not playback the user had already
        /// paused before the interruption.
        var pausedForInterruption = false
        /// Transport handle the remote commands and interruption/route handlers
        /// forward to. Set in `setupRemoteCommands`.
        var playback: Playback?
    #endif

    public init() {}

    // MARK: - Retained media-control values

    /// Translate core's resolved media-control presentation onto the platform's
    /// single Now Playing surface.
    public func applyMediaControlValues(
        _ values: BridgeMediaControlValues,
        appHandle: AppHandle
    ) {
        switch values.playback {
        case .preview(
            let target,
            let durationMs,
            let positionMs,
            let isPlaying
        ):
            #if os(macOS)
                updatePreviewNowPlaying(
                    target: target,
                    durationMs: durationMs,
                    isPlaying: isPlaying
                )
                updatePreviewPosition(positionMs: positionMs)
            #endif
        case .library(let state, let position, _):
            #if os(macOS)
                let previewEnded = isShowingPreview
                isShowingPreview = false
                if previewEnded, case .loading(_, nil) = state {
                    clearNowPlaying()
                }
            #endif
            updateNowPlaying(state: state, appHandle: appHandle)
            if let position {
                updatePosition(
                    positionMs: position.positionMs,
                    durationMs: position.durationMs
                )
            }
        }
    }

    // MARK: - Library Now Playing

    /// Push the Now Playing info for a library playback state. A `.stopped`
    /// state clears; a bare `.loading` without resolved metadata keeps the
    /// current info on screen.
    public func updateNowPlaying(
        state: BridgePlaybackValueState,
        appHandle: AppHandle
    ) {
        #if os(iOS)
            applyAudioSessionTransition(for: state)
        #endif
        if case .stopped = state {
            clearNowPlaying()
            #if os(iOS)
                endPlaybackSession()
            #endif
            return
        }
        if let metadata = Self.libraryMetadata(for: state) {
            setNowPlaying(
                metadata,
                appHandle: appHandle,
                on: MPNowPlayingInfoCenter.default()
            )
        }
    }

    /// The Now Playing fields for a playable state, or `nil` when there's
    /// nothing to push (a bare loading event whose target metadata hasn't
    /// landed). The `playing` rate is 1.0; `paused` and a resolved `loading`
    /// target are 0.0 (no audio is flowing yet for the latter).
    private static func libraryMetadata(
        for state: BridgePlaybackValueState
    ) -> NowPlayingMetadata? {
        switch state {
        case .playing(
            _,
            let trackTitle,
            let artistNames,
            _,
            _,
            let albumTitle,
            let coverImage,
            let durationMs
        ),
            .paused(
                _,
                let trackTitle,
                let artistNames,
                _,
                _,
                let albumTitle,
                let coverImage,
                let durationMs,
                _
            ):
            let playbackRate: Double =
                if case .playing = state {
                    1.0
                }
                else {
                    0.0
                }
            return NowPlayingMetadata(
                trackTitle: trackTitle,
                artistNames: artistNames,
                albumTitle: albumTitle,
                durationMs: durationMs,
                coverImage: coverImage,
                playbackRate: playbackRate
            )

        case .loading(_, let track):
            return track.map { track in
                NowPlayingMetadata(
                    trackTitle: track.trackTitle,
                    artistNames: track.artistNames,
                    albumTitle: track.albumTitle,
                    durationMs: track.durationMs,
                    coverImage: track.coverImage,
                    playbackRate: 0.0
                )
            }

        case .stopped:
            return nil
        }
    }

    /// Push the Now Playing info for a library track. Duration is tracked
    /// through `trackDuration`, which enables the scrubber only while a track
    /// duration is known.
    func setNowPlaying(
        _ metadata: NowPlayingMetadata,
        appHandle: AppHandle,
        on infoCenter: MPNowPlayingInfoCenter
    ) {
        var info = infoCenter.nowPlayingInfo ?? [:]
        info[MPMediaItemPropertyTitle] = metadata.trackTitle
        info[MPMediaItemPropertyArtist] = metadata.artistNames
        info[MPMediaItemPropertyAlbumTitle] = metadata.albumTitle
        trackDuration(metadata.durationMs, into: &info)
        info[MPNowPlayingInfoPropertyPlaybackRate] = metadata.playbackRate
        applyArtwork(
            image: metadata.coverImage,
            appHandle: appHandle,
            into: &info
        )
        infoCenter.nowPlayingInfo = info
    }

    public func updatePosition(positionMs: UInt64, durationMs: UInt64) {
        let infoCenter = MPNowPlayingInfoCenter.default()
        guard var info = infoCenter.nowPlayingInfo else {
            return
        }
        info[MPNowPlayingInfoPropertyElapsedPlaybackTime] =
            Double(positionMs) / 1000.0
        // Re-track here too: core may only learn the duration once decoding
        // starts, so it can arrive on progress rather than the Playing payload.
        trackDuration(durationMs, into: &info)
        infoCenter.nowPlayingInfo = info
    }

    /// Track the live duration so the scrubber is enabled only while a track
    /// duration is known. Core reports `0` when the length isn't known yet
    /// (e.g. before decode determines it), mirroring Android's `C.TIME_UNSET`.
    /// With a known duration the scrubber is enabled and a seek maps onto it;
    /// with an unknown one we drop the duration and disable the scrub command,
    /// so a drag can't reach a handler that would silently no-op against a
    /// missing duration.
    func trackDuration(
        _ durationMs: UInt64,
        into info: inout [String: Any]
    ) {
        currentDurationMs = durationMs > 0 ? durationMs : nil
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

    public func updateCommandAvailability(hasNext: Bool, hasPrevious: Bool) {
        let center = MPRemoteCommandCenter.shared()
        center.nextTrackCommand.isEnabled = hasNext
        center.previousTrackCommand.isEnabled = hasPrevious
    }

    /// Wipe Now Playing and disable every transport command. Called when
    /// playback stops or the library is torn down.
    public func clearNowPlaying() {
        isShowingPreview = false
        currentDurationMs = nil
        clearArtworkLoad()
        MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
        // Nothing is playing, so disable the transport — leaving next/previous
        // enabled invites taps core ignores — and drop the scrubber (one owner).
        updateCommandAvailability(hasNext: false, hasPrevious: false)
        setScrubbingEnabled(false)
        #if os(iOS)
            lastKnownIsPlaying = false
            pausedForInterruption = false
        #endif
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

    /// Shared library-scrub math: map the drag onto the timeline, project the
    /// seek so the position bar jumps immediately, then dispatch the actual
    /// seek. `seekByRatio` is the platform's transport call.
    func handleScrub(
        positionTime: TimeInterval,
        playbackStore: PlaybackStore,
        seekByRatio: (Double) -> Void
    ) -> MPRemoteCommandHandlerStatus {
        guard let ratio = scrubRatio(for: positionTime) else {
            return .commandFailed
        }
        if let snapshot = playbackStore.projectSeek(ratio: ratio) {
            updatePosition(
                positionMs: UInt64(snapshot.positionMs),
                durationMs: snapshot.durationMs
            )
        }
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

    /// Apply cached artwork synchronously if the image reference is unchanged,
    /// else clear the slot and load the cover in the background, writing it into
    /// the Now Playing info when ready. The reference pins the content version,
    /// so replacing a release's cover reloads the artwork.
    private func applyArtwork(
        image: BridgeImageRef?,
        appHandle: AppHandle,
        into info: inout [String: Any]
    ) {
        guard let image else {
            clearArtworkLoad()
            info.removeValue(forKey: MPMediaItemPropertyArtwork)
            return
        }
        if image == cachedArtworkImage, let artwork = cachedArtwork {
            info[MPMediaItemPropertyArtwork] = artwork
            return
        }
        info.removeValue(forKey: MPMediaItemPropertyArtwork)
        artworkTask?.cancel()
        artworkTask = Task { [weak self] in
            await self?.loadArtwork(image: image, appHandle: appHandle)
        }
    }

    private func loadArtwork(image: BridgeImageRef, appHandle: AppHandle) async
    {
        guard
            let bytes = await fetchArtworkBytes(
                image: image,
                appHandle: appHandle
            ),
            let decoded = await decodeArtwork(bytes: bytes, imageId: image.id)
        else {
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
            cachedArtworkImage = image
            cachedArtwork = artwork
            info[MPMediaItemPropertyArtwork] = artwork
            infoCenter.nowPlayingInfo = info
        }
    }

    private func fetchArtworkBytes(
        image: BridgeImageRef,
        appHandle: AppHandle
    ) async -> Data? {
        do {
            guard
                let data = try await appHandle.fetchLibraryImageBytes(
                    image: image
                )
            else {
                logger.debug("No Now Playing artwork for \(image.id)")
                return nil
            }
            return data
        }
        catch is CancellationError {
            return nil
        }
        catch {
            logger.warning(
                "Failed to fetch Now Playing artwork \(image.id): \(error)"
            )
            return nil
        }
    }

    private func decodeArtwork(
        bytes: Data,
        imageId: String
    ) async -> PlatformImage? {
        let scale = await artworkDisplayScale()
        do {
            return try await ImageLoader.load(
                source: .data(bytes),
                size: .fitTo(points: 600),
                displayScale: scale
            )
        }
        catch is CancellationError {
            return nil
        }
        catch {
            logger.warning(
                "Failed to decode Now Playing artwork \(imageId): \(error)"
            )
            return nil
        }
    }

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
        cachedArtworkImage = nil
        cachedArtwork = nil
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
