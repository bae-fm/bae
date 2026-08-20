import AppKit
import BaeKit
import os.log

private let logger = Logger.bae("KeychainUnlockRetry")

/// Re-attempting a library open that the OS keychain refused.
///
/// The refusal is not a failure to report — the key is present and the library
/// is intact, the machine was simply locked when bae asked. So the open is
/// deferred to a screen that waits, and this is what wakes it: the two
/// workspace signals that mean the login keychain just became readable again,
/// plus app activation as a backstop for a state we did not observe.
///
/// Event-driven on purpose. Nothing here polls, and every attempt names what
/// triggered it, so a run of these lines in the log means something is firing
/// that should not rather than a timer ticking.
extension AppDelegate {
    /// Arm the workspace observers. Called from the launch sequence.
    func startWatchingForKeychainUnlock() {
        observeKeychainUnlock()
    }

    /// Re-attempt an open that the OS keychain refused, when something happened
    /// that plausibly unlocked it.
    ///
    /// Guarded on the screen, so it is a no-op in every other state — this never
    /// interrupts an open library, a welcome chooser, or a genuine unlock
    /// prompt. Each attempt is logged with what triggered it: this is an
    /// event-driven retry, and a run of these lines in the log means something
    /// is firing that should not, not that a poll is ticking.
    func retryOpenIfKeychainWasLocked(trigger: String) {
        guard case .keychainLocked(let libraryId) = screen else { return }
        logger.info(
            "Retrying the library open after \(trigger); the OS keychain may be reachable now"
        )
        openLocalLibrary(id: libraryId)
    }

    /// Watch the two workspace signals that mean the login keychain just became
    /// readable again. Registered once, for the process's life — these fire
    /// whether or not a library is open, and the handler decides.
    func observeKeychainUnlock() {
        let center = NSWorkspace.shared.notificationCenter
        for (name, trigger) in [
            (NSWorkspace.screensDidWakeNotification, "the screens waking"),
            (
                NSWorkspace.sessionDidBecomeActiveNotification,
                "the session becoming active"
            ),
        ] {
            center.addObserver(
                forName: name,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.retryOpenIfKeychainWasLocked(trigger: trigger)
                }
            }
        }
    }

    /// Park an open the keychain refused on the waiting screen.
    ///
    /// Deliberately not `loadError`: nothing failed that the user can act on,
    /// and the welcome chooser would offer to restore a library that is
    /// perfectly intact.
    func deferOpenForLockedKeychain(libraryId: String) {
        logger.info("Library open deferred: the OS keychain is locked")
        screen = .keychainLocked(libraryId: libraryId)
    }

    /// The waiting screen's own button, for when the observers were not the
    /// thing that changed.
    func retryKeychainOpen() {
        retryOpenIfKeychainWasLocked(trigger: "the retry button")
    }
}
