#if os(iOS)
    import AVFAudio
    import MediaPlayer
    import os.log

    private let logger = Logger.bae("MediaControlService")

    extension MediaControlService {
        // MARK: - Remote commands + session

        /// Owns the `AVAudioSession` the cpal CoreAudio sink needs (silent
        /// without an active `.playback` session) and registers the lock-screen
        /// / Control-Center remote commands. Transport intents flow into
        /// `Playback`; the resulting events update Now Playing.
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

        /// Activate the shared `.playback` session as a stream starts. Called on
        /// the first `playbackLoading`/`playbackPlaying`, not at library open, so
        /// opening bae doesn't interrupt audio the user has playing in another
        /// app before they hit play. Idempotent — the cpal AudioUnit produces no
        /// sound until this succeeds, so a session the system refused pauses
        /// the stream rather than letting it play silently.
        func beginPlaybackSession() {
            do {
                try activateSession()
            }
            catch {
                logger.warning(
                    "Pausing playback: the system refused the audio session: \(error)"
                )
                playback?.pause()
            }
        }

        /// The production audio session: the shared instance in `.playback`.
        @usableFromInline
        static func activateSharedPlaybackSession() throws {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playback, mode: .default)
            try session.setActive(true)
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

        /// Activate the session unless it is already active; throws the
        /// system's refusal.
        private func activateSession() throws {
            guard !sessionActivated else {
                return
            }
            try activateAudioSession()
            sessionActivated = true
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
                // Re-activate the session, then auto-resume only when the system
                // signals it (`.shouldResume`) and the interruption is what
                // paused us; otherwise core stays paused until the user resumes.
                sessionActivated = false
                do {
                    try activateSession()
                }
                catch {
                    logger.error(
                        "Failed to activate audio session: \(error.localizedDescription)"
                    )
                }
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

        // MARK: - Session/latch transition

        /// Transition the session and interruption latch for a library Now
        /// Playing push, called from `updateNowPlaying(state:appHandle:)` before
        /// the metadata write (preserving session-before-metadata ordering).
        /// `.loading` counts as playing here — it mirrors `NowPlaying.isPlaying`
        /// (`PlaybackStore.swift`) — so an interruption that arrives mid-track
        /// transition still pauses core and auto-resumes on `.ended`.
        /// `.stopped` is handled by the shared clear path instead: nothing to do
        /// here.
        func applyAudioSessionTransition(for state: BridgePlaybackValueState) {
            switch state {
            case .playing, .loading:
                lastKnownIsPlaying = true
                beginPlaybackSession()

            case .paused:
                lastKnownIsPlaying = false

            case .stopped:
                break
            }
        }
    }
#endif
