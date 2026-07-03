import AVFAudio
import MediaPlayer
import UIKit
import os.log

private let logger = Logger.bae("MediaControlService")

/// The lock-screen / Control-Center metadata for the current track: the fields
/// `updateNowPlaying` writes into `MPNowPlayingInfoCenter`, grouped so the call
/// takes one metadata value rather than five positional parameters.
struct NowPlayingMetadata {
    let trackTitle: String
    let artistNames: String
    let albumTitle: String
    let coverImageId: String?
    let durationMs: UInt64
}

/// Bridges playback state to iOS Now Playing (lock screen + Control Center) and
/// owns the `AVAudioSession`. The cpal CoreAudio sink is silent without an
/// active `.playback` session, so this activates one when playback starts and
/// pauses core on interruption (call, alarm) and unplug.
///
/// Transport intent flows the other way: remote commands (play/pause/skip/seek
/// from the lock screen or headphones) call into `Playback`, and the resulting
/// `Playback*` events come back through the reducer to update Now Playing — the
/// same single source of truth the in-app bar reads.
final class MediaControlService: @unchecked Sendable {
    private var sessionActivated = false
    private var observersRegistered = false
    private var currentDurationMs: UInt64?
    /// Mirrors `changePlaybackPositionCommand.isEnabled` so the scrubber is
    /// toggled (and its disable logged) only on a real change — the duration
    /// path runs on every 200 ms progress tick, so writing each time would spam.
    private var scrubbingEnabled = false
    private var cachedArtworkImageId: String?
    private var cachedArtwork: MPMediaItemArtwork?
    private var artworkTask: Task<Void, Never>?

    /// Latched from the most recent Now Playing update so the interruption
    /// handler knows whether playback was active when a call/alarm began.
    private var lastKnownIsPlaying = false
    /// Set when an interruption paused active playback, so `.ended` only
    /// auto-resumes playback we paused — not playback the user had already
    /// paused before the interruption.
    private var pausedForInterruption = false

    /// Transport handle the remote commands and interruption/route handlers
    /// forward to. Set in `setupRemoteCommands`.
    private var playback: Playback?

    // MARK: - Remote commands + session

    func setupRemoteCommands(
        playback: Playback,
        playbackStore: PlaybackStore
    ) {
        self.playback = playback
        registerSessionObservers()

        let center = MPRemoteCommandCenter.shared()

        // The command center is process-global. A library re-open builds a new
        // `MediaControlService` whose targets would stack on top of the prior
        // instance's stale closures, so clear them before re-adding.
        center.playCommand.removeTarget(nil)
        center.pauseCommand.removeTarget(nil)
        center.togglePlayPauseCommand.removeTarget(nil)
        center.nextTrackCommand.removeTarget(nil)
        center.previousTrackCommand.removeTarget(nil)
        center.changePlaybackPositionCommand.removeTarget(nil)

        center.playCommand.addTarget { _ in
            playback.resume()
            return .success
        }
        center.pauseCommand.addTarget { _ in
            playback.pause()
            return .success
        }
        center.togglePlayPauseCommand.addTarget { _ in
            playback.togglePlayPause()
            return .success
        }
        center.nextTrackCommand.addTarget { _ in
            playback.nextTrack()
            return .success
        }
        center.previousTrackCommand.addTarget { _ in
            playback.previousTrack()
            return .success
        }
        center.changePlaybackPositionCommand.addTarget { [weak self] event in
            guard let self,
                let positionEvent = event
                    as? MPChangePlaybackPositionCommandEvent
            else {
                return .noActionableNowPlayingItem
            }
            guard let durationMs = currentDurationMs, durationMs > 0 else {
                logger.error("Scrubber seek ignored: no current duration tracked")
                return .commandFailed
            }
            let ratio = positionEvent.positionTime / (Double(durationMs) / 1000.0)
            if let snapshot = playbackStore.projectSeek(ratio: ratio) {
                updatePosition(
                    positionMs: snapshot.positionMs,
                    durationMs: snapshot.durationMs
                )
            }
            playback.seekByRatio(ratio)
            return .success
        }
    }

    /// Activate the shared `.playback` session as a stream starts. Called on
    /// the first `playbackLoading`/`playbackPlaying`, not at library open, so
    /// opening bae doesn't interrupt audio the user has playing in another app
    /// before they hit play. Idempotent — the cpal AudioUnit produces no sound
    /// until this succeeds.
    func beginPlaybackSession() {
        activateSession()
    }

    /// Deactivate the session on `playbackStopped` and notify other apps so
    /// they can resume. Re-activated on the next `beginPlaybackSession()`.
    func endPlaybackSession() {
        guard sessionActivated else {
            return
        }
        do {
            try AVAudioSession.sharedInstance()
                .setActive(
                    false,
                    options: .notifyOthersOnDeactivation
                )
            sessionActivated = false
        }
        catch {
            logger.error(
                "Failed to deactivate audio session: \(error.localizedDescription)"
            )
        }
    }

    private func activateSession() {
        guard !sessionActivated else {
            return
        }
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.playback, mode: .default)
            try session.setActive(true)
            sessionActivated = true
        }
        catch {
            logger.error(
                "Failed to activate audio session: \(error.localizedDescription)"
            )
        }
    }

    private func registerSessionObservers() {
        guard !observersRegistered else {
            return
        }
        observersRegistered = true
        let center = NotificationCenter.default
        center.addObserver(
            self,
            selector: #selector(handleInterruption(_:)),
            name: AVAudioSession.interruptionNotification,
            object: nil
        )
        center.addObserver(
            self,
            selector: #selector(handleRouteChange(_:)),
            name: AVAudioSession.routeChangeNotification,
            object: nil
        )
    }

    @objc
    private func handleInterruption(_ notification: Notification) {
        guard
            let info = notification.userInfo,
            let rawType = info[AVAudioSessionInterruptionTypeKey] as? UInt,
            let type = AVAudioSession.InterruptionType(rawValue: rawType)
        else {
            return
        }
        switch type {
        case .began:
            // A call / alarm took the session — pause core so it doesn't keep
            // decoding into a dead output. Remember whether we actually paused
            // active playback, so `.ended` only auto-resumes what we paused.
            pausedForInterruption = lastKnownIsPlaying
            if lastKnownIsPlaying {
                playback?.pause()
            }
        case .ended:
            // Re-activate the session, then auto-resume only when the system
            // signals it (`.shouldResume`) and the interruption is what paused
            // us; otherwise core stays paused until the user resumes.
            sessionActivated = false
            activateSession()
            let shouldResume =
                (info[AVAudioSessionInterruptionOptionKey] as? UInt)
                .map(AVAudioSession.InterruptionOptions.init(rawValue:))?
                .contains(.shouldResume) ?? false
            if pausedForInterruption, shouldResume {
                playback?.resume()
            }
            pausedForInterruption = false
        @unknown default:
            break
        }
    }

    @objc
    private func handleRouteChange(_ notification: Notification) {
        guard
            let info = notification.userInfo,
            let rawReason = info[AVAudioSessionRouteChangeReasonKey] as? UInt,
            let reason = AVAudioSession.RouteChangeReason(rawValue: rawReason)
        else {
            return
        }
        // Headphones unplugged / Bluetooth disconnected — pause, matching the
        // "becoming noisy" behavior users expect.
        if reason == .oldDeviceUnavailable {
            playback?.pause()
        }
    }

    // MARK: - Now Playing info

    func updateNowPlaying(
        _ metadata: NowPlayingMetadata,
        isPlaying: Bool,
        appHandle: AppHandle
    ) {
        lastKnownIsPlaying = isPlaying
        let infoCenter = MPNowPlayingInfoCenter.default()
        var info = infoCenter.nowPlayingInfo ?? [:]
        info[MPMediaItemPropertyTitle] = metadata.trackTitle
        info[MPMediaItemPropertyArtist] = metadata.artistNames
        info[MPMediaItemPropertyAlbumTitle] = metadata.albumTitle
        trackDuration(metadata.durationMs, into: &info)
        info[MPNowPlayingInfoPropertyPlaybackRate] = isPlaying ? 1.0 : 0.0
        applyArtwork(imageId: metadata.coverImageId, appHandle: appHandle, into: &info)
        infoCenter.nowPlayingInfo = info
    }

    func updatePosition(positionMs: UInt64, durationMs: UInt64) {
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

    /// Track the live duration and keep the lock-screen scrubber honest. Core
    /// reports `0` when the length isn't known yet (e.g. before decode
    /// determines it), mirroring Android's `C.TIME_UNSET`. With a known
    /// duration the scrubber is enabled and a seek maps onto it; with an
    /// unknown one we drop the duration and disable the scrub command, so a
    /// drag can't reach a handler that would silently no-op against a missing
    /// duration.
    private func trackDuration(
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

    /// Single owner of the lock-screen scrub command's enabled state, so the
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

    func updateCommandAvailability(hasNext: Bool, hasPrevious: Bool) {
        let center = MPRemoteCommandCenter.shared()
        center.nextTrackCommand.isEnabled = hasNext
        center.previousTrackCommand.isEnabled = hasPrevious
    }

    func clearNowPlaying() {
        currentDurationMs = nil
        lastKnownIsPlaying = false
        pausedForInterruption = false
        artworkTask?.cancel()
        artworkTask = nil
        cachedArtworkImageId = nil
        cachedArtwork = nil
        MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
        // Nothing is playing, so disable the lock-screen / Control-Center
        // transport — leaving next/previous enabled invites taps core ignores.
        updateCommandAvailability(hasNext: false, hasPrevious: false)
        // No timeline to scrub either, so drop the scrubber (one owner).
        setScrubbingEnabled(false)
    }
}

extension MediaControlService {
    // MARK: - Artwork

    /// Apply cached artwork synchronously if `imageId` is unchanged, else clear
    /// the slot and load the on-disk cover in the background, writing it into
    /// the Now Playing info when ready.
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
        info.removeValue(forKey: MPMediaItemPropertyArtwork)
        artworkTask?.cancel()
        artworkTask = Task { [weak self] in
            await self?.loadArtwork(imageId: imageId, appHandle: appHandle)
        }
    }

    private func loadArtwork(imageId: String, appHandle: AppHandle) async {
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
        let image: UIImage? =
            await Task.detached(priority: .userInitiated) {
                UIImage(data: bytes)
            }
            .value
        guard !Task.isCancelled else {
            return
        }
        guard let image else {
            logger.warning("Failed to decode Now Playing artwork \(imageId)")
            return
        }
        let box = SendableImage(image)
        let artwork = MPMediaItemArtwork(boundsSize: image.size) { _ in
            box.value
        }
        cachedArtworkImageId = imageId
        cachedArtwork = artwork
        let infoCenter = MPNowPlayingInfoCenter.default()
        var info = infoCenter.nowPlayingInfo ?? [:]
        info[MPMediaItemPropertyArtwork] = artwork
        infoCenter.nowPlayingInfo = info
    }
}

/// `UIImage` is documented thread-safe for reads after construction, but isn't
/// `Sendable`. Box it so the artwork request closure can be passed across the
/// concurrency boundary `MPMediaItemArtwork` expects.
private final class SendableImage: @unchecked Sendable {
    let value: UIImage
    init(_ value: UIImage) {
        self.value = value
    }
}
