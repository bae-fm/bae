import BaeKit
import SwiftUI

/// First-run and add-a-library entry point. A thin switcher over the flows —
/// choose, and restore or join by code — each its own view with its own
/// state. Because the switch branches are distinct view types, SwiftUI gives
/// each flow fresh `@State` every time it appears, so nothing leaks between
/// flows.
struct WelcomeView: View {
    /// A created, restored or joined library's open store, with the error a
    /// partly finished restore or join reported.
    let onLibraryReady: @MainActor (OpenedLibrary, BridgeError?) -> Void
    /// Open one of this Mac's libraries by id.
    let onOpenLibrary: (String) -> Void
    /// A failed library open, shown as the chooser's inline callout. Only the
    /// first-run bootstrap carries it; a flow a menu item opens directly shows
    /// its own errors.
    let loadError: DisplayError?
    let canDeleteActiveLibrary: Bool

    enum Mode {
        case choose
        /// Restore by a typed or scanned code, or from a code coven saved in
        /// iCloud Keychain.
        case restore(BridgeSavedRestoreCode?)
        case join
    }

    @Environment(LibrarySetup.self)
    private var setup

    @State
    private var mode: Mode

    init(
        onLibraryReady:
            @escaping @MainActor (OpenedLibrary, BridgeError?) -> Void,
        onOpenLibrary: @escaping (String) -> Void,
        loadError: DisplayError? = nil,
        initialMode: Mode = .choose,
        canDeleteActiveLibrary: Bool
    ) {
        self.onLibraryReady = onLibraryReady
        self.onOpenLibrary = onOpenLibrary
        self.loadError = loadError
        self.canDeleteActiveLibrary = canDeleteActiveLibrary
        _mode = State(initialValue: initialMode)
    }

    var body: some View {
        switch mode {
        case .choose:
            WelcomeChooseView(
                loadError: loadError,
                canDeleteActiveLibrary: canDeleteActiveLibrary,
                onLibraryReady: onLibraryReady,
                onOpenLibrary: onOpenLibrary,
                onRestoreSaved: { mode = .restore($0) },
                onJoin: { mode = .join },
                onRestore: { mode = .restore(nil) }
            )
        case .restore(let saved):
            BootstrapCodeView(
                form: codeForm(.restore(saved)),
                onBack: { mode = .choose }
            )
        case .join:
            BootstrapCodeView(
                form: codeForm(.join),
                onBack: { mode = .choose }
            )
        }
    }

    /// The code screen keeps the first form it is handed for as long as it
    /// shows; switching flows gives it a fresh one.
    private func codeForm(
        _ entry: BootstrapCodeForm.Entry
    ) -> BootstrapCodeForm {
        setup.bootstrapForm(entry: entry, onLibraryReady: onLibraryReady)
    }
}

#if DEBUG
    // MARK: - Previews

    /// The flows the switcher lands on, each in the welcome window's chrome
    /// with the populated `LibrarySetup` so no flow touches the real library
    /// directory or keychain.
    #Preview("Choose") {
        PreviewScenes.welcome()
    }

    #Preview("Restore") {
        PreviewScenes.welcomeRestore()
    }

    #Preview("Join") {
        WelcomeWindowChrome {
            WelcomeView(
                onLibraryReady: { _, _ in },
                onOpenLibrary: { _ in },
                initialMode: .join,
                canDeleteActiveLibrary: true
            )
        }
        .environment(PreviewData.welcomeSetup())
    }
#endif
