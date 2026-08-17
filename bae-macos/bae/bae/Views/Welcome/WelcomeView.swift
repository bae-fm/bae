import BaeKit
import SwiftUI

/// First-run and add-a-library entry point. A thin switcher over the three
/// flows — choose / restore / join — each its own view with its own state.
/// Because the switch branches are distinct view types, SwiftUI gives each
/// flow fresh `@State` every time it appears, so nothing leaks between flows.
struct WelcomeView: View {
    let onLibraryReady: (BridgeLibrary) -> Void
    /// A failed library open, shown as the chooser's inline callout. Only the
    /// first-run bootstrap carries it; the deep-link initializer (restore-from-
    /// code) lands on a flow with its own error line, so it stays nil there.
    let loadError: DisplayError?
    let canDeleteActiveLibrary: Bool

    @Environment(LibrarySetup.self)
    private var setup

    enum Mode {
        case choose
        case restore
        case join(BridgePendingDevicePairingJoin?)
    }

    @State
    private var mode: Mode
    @State
    private var pendingLoadError: DisplayError?

    /// Default initializer used by first-run flow. Lands on `.choose`.
    init(
        onLibraryReady: @escaping (BridgeLibrary) -> Void,
        loadError: DisplayError? = nil,
        canDeleteActiveLibrary: Bool
    ) {
        self.onLibraryReady = onLibraryReady
        self.loadError = loadError
        self.canDeleteActiveLibrary = canDeleteActiveLibrary
        self._mode = State(initialValue: .choose)
        self._pendingLoadError = State(initialValue: nil)
    }

    /// Initializer used by the sidebar's "+ Add..." menu when the user
    /// has already picked a specific flow (Restore from code). Skips the
    /// chooser and lands directly on the requested mode.
    init(
        onLibraryReady: @escaping (BridgeLibrary) -> Void,
        initialMode: Mode,
        canDeleteActiveLibrary: Bool
    ) {
        self.onLibraryReady = onLibraryReady
        self.loadError = nil
        self.canDeleteActiveLibrary = canDeleteActiveLibrary
        self._mode = State(initialValue: initialMode)
        self._pendingLoadError = State(initialValue: nil)
    }

    var body: some View {
        Group {
            switch mode {
            case .choose:
                WelcomeChooseView(
                    loadError: pendingLoadError ?? loadError,
                    canDeleteActiveLibrary: canDeleteActiveLibrary,
                    onLibraryReady: onLibraryReady,
                    onJoin: { mode = .join(nil) },
                    onRestore: { mode = .restore },
                )
            case .restore:
                RestoreFromCloudView(
                    onLibraryReady: onLibraryReady,
                    onBack: { mode = .choose },
                )
            case .join(let pending):
                JoinLibraryView(
                    onLibraryReady: onLibraryReady,
                    onBack: { mode = .choose },
                    pending: pending
                )
            }
        }
        .task {
            guard case .choose = mode else { return }
            let pendingDevicePairingJoin = setup.pendingDevicePairingJoin
            do {
                if let pending = try await DetachedWork.run({
                    try pendingDevicePairingJoin()
                }) {
                    mode = .join(pending)
                }
            }
            catch {
                pendingLoadError = DisplayError(error)
            }
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// The three flows the switcher lands on, each in the welcome window's
    /// chrome with the populated `LibrarySetup` so no flow touches the real
    /// library directory or keychain.
    #Preview("Choose") {
        PreviewScenes.welcome()
    }

    #Preview("Restore") {
        PreviewScenes.welcomeRestore()
    }

    #Preview("Join") {
        WelcomeWindowChrome {
            WelcomeView(
                onLibraryReady: { _ in },
                initialMode: .join(nil),
                canDeleteActiveLibrary: true
            )
        }
        .environment(PreviewData.welcomeSetup())
    }
#endif
