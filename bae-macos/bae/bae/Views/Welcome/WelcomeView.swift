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
    let loadError: String?

    enum Mode {
        case choose
        case restore
        case join
    }

    @State
    private var mode: Mode

    /// Default initializer used by first-run flow. Lands on `.choose`.
    init(
        onLibraryReady: @escaping (BridgeLibrary) -> Void,
        loadError: String? = nil
    ) {
        self.onLibraryReady = onLibraryReady
        self.loadError = loadError
        self._mode = State(initialValue: .choose)
    }

    /// Initializer used by the sidebar's "+ Add..." menu when the user
    /// has already picked a specific flow (Restore from code). Skips the
    /// chooser and lands directly on the requested mode.
    init(
        onLibraryReady: @escaping (BridgeLibrary) -> Void,
        initialMode: Mode
    ) {
        self.onLibraryReady = onLibraryReady
        self.loadError = nil
        self._mode = State(initialValue: initialMode)
    }

    var body: some View {
        switch mode {
        case .choose:
            WelcomeChooseView(
                loadError: loadError,
                onLibraryReady: onLibraryReady,
                onJoin: { mode = .join },
                onRestore: { mode = .restore },
            )
        case .restore:
            RestoreFromCloudView(
                onLibraryReady: onLibraryReady,
                onBack: { mode = .choose },
            )
        case .join:
            JoinLibraryView(
                onLibraryReady: onLibraryReady,
                onBack: { mode = .choose },
            )
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
            WelcomeView(onLibraryReady: { _ in }, initialMode: .join)
        }
        .environment(PreviewData.welcomeSetup)
    }
#endif
