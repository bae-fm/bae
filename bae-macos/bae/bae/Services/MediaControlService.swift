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

/// Bridges playback state to macOS Now Playing (Control Center widget + media keys).
final class MediaControlService: @unchecked Sendable {
    private var commandsRegistered = false
    private var cachedArtworkImageId: String?
    private var cachedArtwork: MPMediaItemArtwork?
    private var artworkTask: Task<Void, Never>?
    private var isShowingPreview = false
    private var currentDurationMs: UInt64?

    func setupRemoteCommands(playback: Playback, previewAudio: PreviewAudio) {
        guard !commandsRegistered else {
            return
        }
        commandsRegistered = true

        let center = MPRemoteCommandCenter.shared()
        registerPlayPauseCommands(
            center: center,
            playback: playback,
            previewAudio: previewAudio
        )
        registerTrackCommands(
            center: center,
            playback: playback,
            previewAudio: previewAudio
        )
        registerScrubCommand(
            center: center,
            playback: playback,
            previewAudio: previewAudio
        )
    }

    private func registerPlayPauseCommands(
        center: MPRemoteCommandCenter,
        playback: Playback,
        previewAudio: PreviewAudio
    ) {
        center.playCommand.addTarget { [weak self] _ in
            guard let self else {
                return .noActionableNowPlayingItem
            }
            if isShowingPreview {
                previewAudio.previewTogglePause()
            }
            else {
                playback.resume()
            }
            return .success
        }

        center.pauseCommand.addTarget { [weak self] _ in
            guard let self else {
                return .noActionableNowPlayingItem
            }
            if isShowingPreview {
                previewAudio.previewTogglePause()
            }
            else {
                playback.pause()
            }
            return .success
        }

        center.togglePlayPauseCommand.addTarget { [weak self] _ in
            guard let self else {
                return .noActionableNowPlayingItem
            }
            if isShowingPreview {
                previewAudio.previewTogglePause()
            }
            else {
                playback.togglePlayPause()
            }
            return .success
        }
    }

    private func registerTrackCommands(
        center: MPRemoteCommandCenter,
        playback: Playback,
        previewAudio: PreviewAudio
    ) {
        center.nextTrackCommand.addTarget { _ in
            previewAudio.previewStop()
            playback.nextTrack()
            return .success
        }

        center.previousTrackCommand.addTarget { _ in
            previewAudio.previewStop()
            playback.previousTrack()
            return .success
        }
    }

    private func registerScrubCommand(
        center: MPRemoteCommandCenter,
        playback: Playback,
        previewAudio: PreviewAudio
    ) {
        center.changePlaybackPositionCommand.addTarget { [weak self] event in
            guard let self,
                let positionEvent = event
                    as? MPChangePlaybackPositionCommandEvent
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
                positionEvent.positionTime / (Double(durationMs) / 1000.0)
            if isShowingPreview {
                previewAudio.previewSeekByRatio(ratio)
            }
            else {
                playback.seekByRatio(ratio)
            }
            return .success
        }
    }

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
            currentDurationMs = nil
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
            cachedArtworkImageId = nil
            cachedArtwork = nil
            artworkTask?.cancel()
            artworkTask = nil
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
                let data = try await appHandle.fetchImageBytes(imageId: imageId)
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
            logger.warning("Failed to fetch Now Playing artwork: \(error)")
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
            logger.warning("Failed to decode Now Playing artwork: \(error)")
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
            cachedArtworkImageId = imageId
            cachedArtwork = artwork
            let infoCenter = MPNowPlayingInfoCenter.default()
            var info = infoCenter.nowPlayingInfo ?? [:]
            info[MPMediaItemPropertyArtwork] = artwork
            infoCenter.nowPlayingInfo = info
        }
    }
}

private final class SendableNSImage: @unchecked Sendable {
    let value: NSImage
    init(_ value: NSImage) {
        self.value = value
    }
}
