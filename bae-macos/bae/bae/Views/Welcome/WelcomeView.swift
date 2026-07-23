import BaeKit
import SwiftUI

/// First-run and add-a-library entry point. A thin switcher over the three
/// flows — choose / restore / join — each its own view with its own state.
/// Because the switch branches are distinct view types, SwiftUI gives each
/// flow fresh `@State` every time it appears, so nothing leaks between flows.
struct WelcomeView: View {
    let onLibraryReady: (BridgeLibrary) -> Void

    enum Mode {
        case choose
        case restore
        case join
    }

    @State
    private var mode: Mode

    /// Default initializer used by first-run flow. Lands on `.choose`.
    init(onLibraryReady: @escaping (BridgeLibrary) -> Void) {
        self.onLibraryReady = onLibraryReady
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
        self._mode = State(initialValue: initialMode)
    }

    var body: some View {
        switch mode {
        case .choose:
            WelcomeChooseView(
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
