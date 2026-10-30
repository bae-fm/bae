#if os(iOS)
    import AVFAudio
    import os.log

    private let logger = Logger.bae("PlaybackAudioSession")

    /// The iOS audio session as the gate core asks before every start of
    /// playback, one per process. A start activates the shared `.playback`
    /// session — the cpal CoreAudio sink is silent without it — and a session
    /// the system refuses, while a call holds it, keeps playback from starting.
    /// Core releases it once playback stops, so other apps can resume. Core
    /// calls both methods on its playback thread, one at a time.
    final class PlaybackAudioSession: BridgeAudioFocus,
        @unchecked Sendable
    {
        /// Whether a start activated the session since the last release. Only
        /// core's playback thread touches it.
        private var activated = false

        /// Activate the session before playback starts. Activating it again
        /// while active succeeds, and one the system took back for a call is
        /// refused until the call ends.
        func request() -> BridgeAudioFocusAnswer {
            do {
                let session = AVAudioSession.sharedInstance()
                try session.setCategory(.playback, mode: .default)
                try session.setActive(true)
                activated = true
                return .granted
            }
            catch {
                return .refused(reason: "\(error)")
            }
        }

        /// Deactivate the session once playback stops, telling other apps they
        /// can resume.
        func release() {
            guard activated else {
                return
            }
            activated = false
            do {
                try AVAudioSession.sharedInstance()
                    .setActive(false, options: .notifyOthersOnDeactivation)
            }
            catch {
                logger.error(
                    "Failed to deactivate the audio session: \(String(reflecting: error))"
                )
            }
        }
    }
#endif
