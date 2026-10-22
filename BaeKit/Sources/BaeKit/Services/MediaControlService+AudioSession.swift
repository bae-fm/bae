#if os(iOS)
    import AVFAudio
    import MediaPlayer

    extension MediaControlService {
        // MARK: - Remote commands + session

        /// Follows what the system does to the audio session — a call or an
        /// alarm interrupting it, headphones coming out — and registers the
        /// lock-screen / Control-Center remote commands. Transport intents flow
        /// into `Playback`; the resulting events update Now Playing. Core
        /// activates the session before every start of playback and releases
        /// it once playback stops (`PlaybackAudioSession`).
        public func setupRemoteCommands(
            playback: Playback,
            playbackStore: PlaybackStore
        ) {
            self.playback = playback
            registerSessionObservers()
            // Each transport command runs one `Playback` call and reports
            // success; wrap the method so the five share one shape.
            func transport(
                _ command: @escaping () -> Void
            ) -> () -> MPRemoteCommandHandlerStatus {
                {
                    command()
                    return .success
                }
            }
            registerTransportCommands(
                center: MPRemoteCommandCenter.shared(),
                actions: TransportActions(
                    play: transport(playback.resume),
                    pause: transport(playback.pause),
                    toggle: transport({
                        playback.playPause(for: playbackStore.nowPlaying)
                    }),
                    next: transport(playback.nextTrack),
                    previous: transport(playback.previousTrack),
                    seek: { [weak self] positionTime in
                        self?
                            .handleScrub(
                                positionTime: positionTime,
                                playbackStore: playbackStore,
                                seekByRatio: playback.seekByRatio
                            ) ?? .noActionableNowPlayingItem
                    }
                )
            )
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
                // A call / alarm took the session — pause core so it doesn't
                // keep decoding into a dead output. Remember whether we actually
                // paused active playback, so `.ended` only auto-resumes what we
                // paused.
                pausedForInterruption = lastKnownIsPlaying
                if lastKnownIsPlaying {
                    playback?.pause()
                }
            case .ended:
                // Auto-resume only when the system signals it (`.shouldResume`)
                // and the interruption is what paused us; otherwise core stays
                // paused until the user resumes. Resuming asks core, which
                // activates the session again before anything plays.
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
                let rawReason = info[AVAudioSessionRouteChangeReasonKey]
                    as? UInt,
                let reason = AVAudioSession.RouteChangeReason(
                    rawValue: rawReason
                )
            else {
                return
            }
            // Headphones unplugged / Bluetooth disconnected — pause, matching the
            // "becoming noisy" behavior users expect.
            if reason == .oldDeviceUnavailable {
                playback?.pause()
            }
        }

        // MARK: - Interruption latch

        /// Transition the interruption latch for a library Now Playing push,
        /// called from `updateNowPlaying(state:appHandle:)` before the metadata
        /// write.
        /// `.loading` counts as playing here — it mirrors `NowPlaying.isPlaying`
        /// (`PlaybackStore.swift`) — so an interruption that arrives mid-track
        /// transition still pauses core and auto-resumes on `.ended`.
        /// `.stopped` is handled by the shared clear path instead: nothing to do
        /// here.
        func applyInterruptionLatch(for state: BridgePlaybackValueState) {
            switch state {
            case .playing, .loading:
                lastKnownIsPlaying = true

            case .paused:
                lastKnownIsPlaying = false

            case .stopped:
                break
            }
        }
    }
#endif
