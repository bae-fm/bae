#if os(macOS)
    import MediaPlayer
    import os.log

    private let logger = Logger.bae("MediaControlService")

    /// The live transport targets a registered command set dispatches to. Weak
    /// so the service (process-global command center) doesn't keep a torn-down
    /// library's playback graph alive.
    final class ActiveMediaSession {
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

    extension MediaControlService {
        // MARK: - Session lifecycle

        /// Bind the command center to a library's transport graph and register
        /// the remote commands (which dispatch to library or import-preview
        /// playback depending on what currently owns Now Playing).
        public func activate(
            playback: Playback,
            previewAudio: PreviewAudio,
            playbackStore: PlaybackStore
        ) {
            activeSession = ActiveMediaSession(
                playback: playback,
                previewAudio: previewAudio,
                playbackStore: playbackStore
            )
            // Each transport command dispatches to preview or library playback
            // depending on what owns Now Playing; only the method pair varies.
            func command(
                preview: @escaping (PreviewAudio) -> Void,
                library: @escaping (Playback) -> Void
            ) -> () -> MPRemoteCommandHandlerStatus {
                { [weak self] in
                    self?
                        .handlePlaybackCommand(
                            preview: preview,
                            library: library
                        ) ?? .noActionableNowPlayingItem
                }
            }
            registerTransportCommands(
                center: MPRemoteCommandCenter.shared(),
                actions: TransportActions(
                    play: command(
                        preview: { $0.previewTogglePause() },
                        library: { $0.resume() }
                    ),
                    pause: command(
                        preview: { $0.previewTogglePause() },
                        library: { $0.pause() }
                    ),
                    toggle: { [weak self] in
                        self?.handleToggleCommand()
                            ?? .noActionableNowPlayingItem
                    },
                    next: command(
                        preview: { $0.previewStop() },
                        library: { $0.nextTrack() }
                    ),
                    previous: command(
                        preview: { $0.previewStop() },
                        library: { $0.previousTrack() }
                    ),
                    seek: { [weak self] positionTime in
                        self?
                            .handlePreviewOrLibraryScrub(
                                positionTime: positionTime
                            ) ?? .noActionableNowPlayingItem
                    }
                )
            )
        }

        public func deactivate(playbackStore: PlaybackStore) {
            guard let activeSession else {
                logger.error(
                    "Media controls deactivated without an active session"
                )
                return
            }
            guard activeSession.playbackStore === playbackStore else {
                logger.error(
                    "Media controls deactivated for a non-current session"
                )
                return
            }
            self.activeSession = nil
            clearNowPlaying()
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

        private func handleToggleCommand() -> MPRemoteCommandHandlerStatus {
            guard let activeSession,
                let playback = activeSession.playback,
                let previewAudio = activeSession.previewAudio,
                let playbackStore = activeSession.playbackStore
            else {
                return .noActionableNowPlayingItem
            }
            if isShowingPreview {
                previewAudio.previewTogglePause()
            }
            else {
                playback.playPause(for: playbackStore.nowPlaying)
            }
            return .success
        }

        private func handlePreviewOrLibraryScrub(
            positionTime: TimeInterval
        ) -> MPRemoteCommandHandlerStatus {
            guard let activeSession,
                let playback = activeSession.playback,
                let previewAudio = activeSession.previewAudio,
                let playbackStore = activeSession.playbackStore
            else {
                return .noActionableNowPlayingItem
            }
            if isShowingPreview {
                guard let ratio = scrubRatio(for: positionTime) else {
                    return .commandFailed
                }
                previewAudio.previewSeekByRatio(ratio)
                return .success
            }
            return handleScrub(
                positionTime: positionTime,
                playbackStore: playbackStore,
                seekByRatio: playback.seekByRatio
            )
        }

        // MARK: - Preview Now Playing

        func updatePreviewNowPlaying(
            target: BridgePreviewTarget,
            durationMs: UInt64,
            isPlaying: Bool
        ) {
            setPreviewNowPlaying(
                target: target,
                durationMs: durationMs,
                playbackRate: isPlaying ? 1.0 : 0.0
            )
        }

        /// Set Now Playing's elapsed time to the preview's `positionMs`, from
        /// which the system moves it forward at the shown rate.
        func updatePreviewPosition(positionMs: UInt64) {
            let infoCenter = MPNowPlayingInfoCenter.default()
            guard let info = infoCenter.nowPlayingInfo else {
                return
            }
            anchor = ElapsedAnchor(
                positionMs: positionMs,
                rate: rate,
                at: now(),
                seekRevision: nil
            )
            publish(info, on: infoCenter)
        }

        /// Pushes the Now Playing info for a preview clip, unless it shows that
        /// clip at that rate already. `playbackRate` is 1.0 playing, 0.0 paused.
        /// The same clip at a new rate keeps its elapsed time moving from where
        /// it is now; another clip waits for its first position.
        private func setPreviewNowPlaying(
            target: BridgePreviewTarget,
            durationMs: UInt64,
            playbackRate: Double
        ) {
            if case .preview(let shownTarget) = shown, shownTarget == target {
                guard
                    playbackRate != rate || durationMs != currentDurationMs
                else {
                    return
                }
                anchor = anchor?.rebased(rate: playbackRate, at: now())
            }
            else {
                anchor = nil
            }
            shown = .preview(target)
            writtenLibrary = nil
            rate = playbackRate
            var info: [String: Any] = [:]
            info[MPMediaItemPropertyTitle] = previewTitle(from: target.path)
            // A preview clip always has a known length; track it through the
            // shared path so the scrubber is enabled, as with a library track.
            trackDuration(durationMs, into: &info)
            info[MPNowPlayingInfoPropertyPlaybackRate] = playbackRate
            publish(info, on: MPNowPlayingInfoCenter.default())
        }

        private func previewTitle(from path: String) -> String {
            (path as NSString).lastPathComponent
        }
    }
#endif
