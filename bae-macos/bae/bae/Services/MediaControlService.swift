import AppKit
import MediaPlayer
import OSLog

private let logger = Logger.bae("MediaControlService")

/// The Now Playing fields for a library track: what `setNowPlaying` writes into
/// `MPNowPlayingInfoCenter`, grouped so the call takes one metadata value rather
/// than seven positional parameters. `playbackRate` (1.0 playing, 0.0 paused) is
/// the only field that differs between the playing and paused states.
struct NowPlayingMetadata {
    let trackTitle: String
    let artistNames: String
    let albumTitle: String
    let durationMs: UInt64
    let coverImageId: String?
    let playbackRate: Double
}

private final class ActiveMediaSession {
    weak var playback: Playback?
    weak var previewAudio: PreviewAudio?
    weak var playbackStore: PlaybackStore?

    init(
        playback: Playback,
        previewAudio: PreviewAudio,
        playbackStore: PlaybackStore
    ) {
        self.playback = playback
        self.previewAudio = previewAudio
        self.playbackStore = playbackStore
    }
}

/// Bridges playback state to macOS Now Playing (Control Center widget + media keys).
final class MediaControlService: @unchecked Sendable {
    private var commandsRegistered = false
    private var cachedArtworkImageId: String?
    private var cachedArtwork: MPMediaItemArtwork?
    private var artworkTask: Task<Void, Never>?
    private var isShowingPreview = false
    private var currentDurationMs: UInt64?
    private var activeSession: ActiveMediaSession?

    func activate(
        playback: Playback,
        previewAudio: PreviewAudio,
        playbackStore: PlaybackStore
    ) {
        activeSession = ActiveMediaSession(
            playback: playback,
            previewAudio: previewAudio,
            playbackStore: playbackStore
        )

        guard !commandsRegistered else {
            return
        }
        commandsRegistered = true

        let center = MPRemoteCommandCenter.shared()
        registerPlayPauseCommands(center: center)
        registerTrackCommands(center: center)
        registerScrubCommand(center: center)
    }

    func deactivate(playbackStore: PlaybackStore) {
        guard let activeSession else {
            logger.error("Media controls deactivated without an active session")
            return
        }
        guard activeSession.playbackStore === playbackStore else {
            logger.error("Media controls deactivated for a non-current session")
            return
        }
        self.activeSession = nil
        isShowingPreview = false
        clearNowPlaying(on: MPNowPlayingInfoCenter.default())
    }

    private func registerPlayPauseCommands(
        center: MPRemoteCommandCenter
    ) {
        center.playCommand.addTarget { [weak self] _ in
            self?
                .handlePlaybackCommand(
                    preview: { $0.previewTogglePause() },
                    library: { $0.resume() }
                ) ?? .noActionableNowPlayingItem
        }

        center.pauseCommand.addTarget { [weak self] _ in
            self?
                .handlePlaybackCommand(
                    preview: { $0.previewTogglePause() },
                    library: { $0.pause() }
                ) ?? .noActionableNowPlayingItem
        }

        center.togglePlayPauseCommand.addTarget { [weak self] _ in
            self?
                .handlePlaybackCommand(
                    preview: { $0.previewTogglePause() },
                    library: { $0.togglePlayPause() }
                ) ?? .noActionableNowPlayingItem
        }
    }

    private func handlePlaybackCommand(
        preview: (PreviewAudio) -> Void,
        library: (Playback) -> Void
    ) -> MPRemoteCommandHandlerStatus {
        guard let activeSession,
            let playback = activeSession.playback,
            let previewAudio = activeSession.previewAudio
        else {
            return .noActionableNowPlayingItem
        }
        if isShowingPreview {
            preview(previewAudio)
        }
        else {
            library(playback)
        }
        return .success
    }

    private func registerTrackCommands(
        center: MPRemoteCommandCenter
    ) {
        center.nextTrackCommand.addTarget { [weak self] _ in
            self?
                .handlePlaybackCommand(
                    preview: { $0.previewStop() },
                    library: { $0.nextTrack() }
                ) ?? .noActionableNowPlayingItem
        }

        center.previousTrackCommand.addTarget { [weak self] _ in
            self?
                .handlePlaybackCommand(
                    preview: { $0.previewStop() },
                    library: { $0.previousTrack() }
                ) ?? .noActionableNowPlayingItem
        }
    }

    private func registerScrubCommand(
        center: MPRemoteCommandCenter
    ) {
        center.changePlaybackPositionCommand.addTarget { [weak self] event in
            guard
                let positionEvent = event
                    as? MPChangePlaybackPositionCommandEvent
            else {
                return .noActionableNowPlayingItem
            }
            let positionTime = positionEvent.positionTime
            guard let service = self else {
                return .noActionableNowPlayingItem
            }
            nonisolated(unsafe) let commandService = service
            return MainActor.assumeIsolated {
                commandService.handleScrubCommand(positionTime: positionTime)
            }
        }
    }

    @MainActor
    private func handleScrubCommand(
        positionTime: TimeInterval
    ) -> MPRemoteCommandHandlerStatus {
        guard let activeSession,
            let playback = activeSession.playback,
            let previewAudio = activeSession.previewAudio,
            let playbackStore = activeSession.playbackStore
        else {
            return .noActionableNowPlayingItem
        }
        guard let durationMs = currentDurationMs, durationMs > 0 else {
            logger.error(
                "Scrubber seek ignored: no current duration tracked"
            )
            return .commandFailed
        }
        let ratio =
            positionTime / (Double(durationMs) / 1000.0)
        if isShowingPreview {
            previewAudio.previewSeekByRatio(ratio)
        }
        else {
            if let snapshot = playbackStore.projectSeek(ratio: ratio) {
                updatePosition(
                    positionMs: snapshot.positionMs,
                    durationMs: snapshot.durationMs
                )
            }
            playback.seekByRatio(ratio)
        }
        return .success
    }
}

extension MediaControlService {
    // MARK: - Library Now Playing

    func updateNowPlaying(state: BridgePlaybackState, appHandle: AppHandle) {
        guard !isShowingPreview else {
            return
        }
        let infoCenter = MPNowPlayingInfoCenter.default()
        if case .stopped = state {
            clearNowPlaying(on: infoCenter)
            return
        }
        // Loading without resolved metadata keeps the existing info on screen.
        if let metadata = Self.libraryMetadata(for: state) {
            setNowPlaying(metadata, appHandle: appHandle, on: infoCenter)
        }
    }

    /// The Now Playing fields for a playable state, or `nil` when there's
    /// nothing to push (a bare loading event whose target metadata hasn't
    /// landed). The `playing` rate is 1.0; `paused` and a resolved `loading`
    /// target are 0.0 (no audio is flowing yet for the latter).
    private static func libraryMetadata(
        for state: BridgePlaybackState
    ) -> NowPlayingMetadata? {
        switch state {
        case .playing(
            _,
            let trackTitle,
            let artistNames,
            _,
            _,
            let albumTitle,
            let coverImageId,
            let durationMs
        ):
            return NowPlayingMetadata(
                trackTitle: trackTitle,
                artistNames: artistNames,
                albumTitle: albumTitle,
                durationMs: durationMs,
                coverImageId: coverImageId,
                playbackRate: 1.0
            )

        case .paused(
            _,
            let trackTitle,
            let artistNames,
            _,
            _,
            let albumTitle,
            let coverImageId,
            let durationMs
        ):
            return NowPlayingMetadata(
                trackTitle: trackTitle,
                artistNames: artistNames,
                albumTitle: albumTitle,
                durationMs: durationMs,
                coverImageId: coverImageId,
                playbackRate: 0.0
            )

        case .loading(_, let track):
            return track.map { track in
                NowPlayingMetadata(
                    trackTitle: track.trackTitle,
                    artistNames: track.artistNames,
                    albumTitle: track.albumTitle,
                    durationMs: track.durationMs,
                    coverImageId: track.coverImageId,
                    playbackRate: 0.0
                )
            }

        case .stopped:
            return nil
        }
    }

    private func clearNowPlaying(on infoCenter: MPNowPlayingInfoCenter) {
        currentDurationMs = nil
        clearArtworkLoad()
        infoCenter.nowPlayingInfo = nil
        let center = MPRemoteCommandCenter.shared()
        center.nextTrackCommand.isEnabled = false
        center.previousTrackCommand.isEnabled = false
    }

    func updatePosition(positionMs: UInt64, durationMs: UInt64) {
        guard !isShowingPreview else {
            return
        }
        let infoCenter = MPNowPlayingInfoCenter.default()
        guard var info = infoCenter.nowPlayingInfo else {
            return
        }
        info[MPNowPlayingInfoPropertyElapsedPlaybackTime] =
            Double(positionMs) / 1000.0
        info[MPMediaItemPropertyPlaybackDuration] = Double(durationMs) / 1000.0
        infoCenter.nowPlayingInfo = info
    }

    func updateCommandAvailability(hasNext: Bool, hasPrevious: Bool) {
        let center = MPRemoteCommandCenter.shared()
        center.nextTrackCommand.isEnabled = hasNext
        center.previousTrackCommand.isEnabled = hasPrevious
    }

    // MARK: - Private

    /// Pushes the Now Playing info for a library track.
    private func setNowPlaying(
        _ metadata: NowPlayingMetadata,
        appHandle: AppHandle,
        on infoCenter: MPNowPlayingInfoCenter
    ) {
        currentDurationMs = metadata.durationMs
        var info = infoCenter.nowPlayingInfo ?? [:]
        info[MPMediaItemPropertyTitle] = metadata.trackTitle
        info[MPMediaItemPropertyArtist] = metadata.artistNames
        info[MPMediaItemPropertyAlbumTitle] = metadata.albumTitle
        info[MPMediaItemPropertyPlaybackDuration] =
            Double(metadata.durationMs) / 1000.0
        info[MPNowPlayingInfoPropertyPlaybackRate] = metadata.playbackRate
        applyArtwork(
            imageId: metadata.coverImageId,
            appHandle: appHandle,
            into: &info
        )
        infoCenter.nowPlayingInfo = info
    }
}

extension MediaControlService {
    // MARK: - Preview

    func updateNowPlayingForPreview(state: BridgePreviewState) {
        let infoCenter = MPNowPlayingInfoCenter.default()

        switch state {
        case .playing(let path, let durationMs):
            setPreviewNowPlaying(
                path: path,
                durationMs: durationMs,
                playbackRate: 1.0,
                on: infoCenter
            )

        case .paused(let path, let durationMs):
            setPreviewNowPlaying(
                path: path,
                durationMs: durationMs,
                playbackRate: 0.0,
                on: infoCenter
            )

        case .idle:
            isShowingPreview = false
            clearNowPlaying(on: infoCenter)
        }
    }

    func updatePreviewPosition(positionMs: UInt64) {
        guard isShowingPreview else {
            return
        }
        let infoCenter = MPNowPlayingInfoCenter.default()
        guard var info = infoCenter.nowPlayingInfo else {
            return
        }
        info[MPNowPlayingInfoPropertyElapsedPlaybackTime] =
            Double(positionMs) / 1000.0
        infoCenter.nowPlayingInfo = info
    }

    /// Pushes the Now Playing info for a preview clip. `playbackRate` is 1.0
    /// playing, 0.0 paused.
    private func setPreviewNowPlaying(
        path: String,
        durationMs: UInt64,
        playbackRate: Double,
        on infoCenter: MPNowPlayingInfoCenter
    ) {
        isShowingPreview = true
        currentDurationMs = durationMs
        var info: [String: Any] = [:]
        info[MPMediaItemPropertyTitle] = previewTitle(from: path)
        info[MPMediaItemPropertyPlaybackDuration] = Double(durationMs) / 1000.0
        info[MPNowPlayingInfoPropertyPlaybackRate] = playbackRate
        infoCenter.nowPlayingInfo = info
    }

    private func previewTitle(from path: String) -> String {
        (path as NSString).lastPathComponent
    }
}

extension MediaControlService {
    // MARK: - Artwork

    /// Synchronous: applies cached artwork if `imageId` is unchanged,
    /// otherwise clears the slot and kicks off an async load that will
    /// write the artwork into MPNowPlayingInfoCenter when ready.
    private func applyArtwork(
        imageId: String?,
        appHandle: AppHandle,
        into info: inout [String: Any]
    ) {
        guard let imageId else {
            clearArtworkLoad()
            info.removeValue(forKey: MPMediaItemPropertyArtwork)
            return
        }

        if imageId == cachedArtworkImageId, let artwork = cachedArtwork {
            info[MPMediaItemPropertyArtwork] = artwork
            return
        }

        // Cache miss — clear the slot and load in the background.
        info.removeValue(forKey: MPMediaItemPropertyArtwork)
        artworkTask?.cancel()
        artworkTask = Task { [weak self] in
            await self?
                .loadArtworkAsync(
                    imageId: imageId,
                    appHandle: appHandle
                )
        }
    }

    private func loadArtworkAsync(
        imageId: String,
        appHandle: AppHandle
    ) async {
        let bytes: Data
        do {
            guard
                let data = try await appHandle.fetchCoverImageBytes(
                    releaseId: imageId
                )
            else {
                logger.debug("No Now Playing artwork for \(imageId)")
                return
            }
            bytes = data
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.warning(
                "Failed to fetch Now Playing artwork \(imageId): \(error)"
            )
            return
        }
        let scale = await MainActor.run {
            NSScreen.main?.backingScaleFactor ?? 2.0
        }
        let nsImage: NSImage
        do {
            nsImage = try await ImageLoader.load(
                source: .data(bytes),
                size: .fitTo(points: 600),
                displayScale: scale
            )
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.warning(
                "Failed to decode Now Playing artwork \(imageId): \(error)"
            )
            return
        }
        guard !Task.isCancelled else {
            return
        }
        // NSImage isn't Sendable, but it's documented thread-safe for
        // reads after construction. Wrap it so the request handler
        // closure can be passed to MPMediaItemArtwork (which expects a
        // sendable closure under strict concurrency).
        let imageBox = SendableNSImage(nsImage)
        let artwork = MPMediaItemArtwork(boundsSize: nsImage.size) { _ in
            imageBox.value
        }
        await MainActor.run {
            guard !Task.isCancelled else {
                return
            }
            let infoCenter = MPNowPlayingInfoCenter.default()
            guard var info = infoCenter.nowPlayingInfo else {
                return
            }
            cachedArtworkImageId = imageId
            cachedArtwork = artwork
            info[MPMediaItemPropertyArtwork] = artwork
            infoCenter.nowPlayingInfo = info
        }
    }

    private func clearArtworkLoad() {
        artworkTask?.cancel()
        artworkTask = nil
        cachedArtworkImageId = nil
        cachedArtwork = nil
    }
}

private final class SendableNSImage: @unchecked Sendable {
    let value: NSImage
    init(_ value: NSImage) {
        self.value = value
    }
}
