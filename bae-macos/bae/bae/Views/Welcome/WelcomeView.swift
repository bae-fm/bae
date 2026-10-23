import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("WelcomeView")

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
    /// Finish the reset or restore of a library's copy on this Mac that
    /// stopped partway, with the one call that finishes it.
    let onFinishReplacement: (String, BridgeReplacedBy) -> Void
    /// A failed library open or dev reset, shown as the chooser's inline
    /// callout. Only the first-run bootstrap carries it; a flow a menu item
    /// opens directly shows its own errors.
    let loadError: LoadError?
    /// What launch found wrong, shown above `loadError` and never replaced by
    /// it.
    let launchProblems: LaunchProblems
    let canDeleteActiveLibrary: Bool

    enum Mode {
        case choose
        /// Restore by a typed or scanned code, or from a code coven saved in
        /// iCloud Keychain.
        case restore(BridgeSavedRestoreCode?)
        case join
        /// A join this device had under way when bae stopped, continued on
        /// its waiting screen.
        case resume(BridgeUnfinishedJoin)
    }

    @Environment(LibrarySetup.self)
    private var setup

    @State
    private var mode: Mode
    /// Listing the joins under way failed; shown above the chooser.
    @State
    private var resumeError: DisplayError?

    init(
        onLibraryReady:
            @escaping @MainActor (OpenedLibrary, BridgeError?) -> Void,
        onOpenLibrary: @escaping (String) -> Void,
        onFinishReplacement: @escaping (String, BridgeReplacedBy) -> Void,
        loadError: LoadError? = nil,
        launchProblems: LaunchProblems = .none,
        initialMode: Mode = .choose,
        canDeleteActiveLibrary: Bool
    ) {
        self.onLibraryReady = onLibraryReady
        self.onOpenLibrary = onOpenLibrary
        self.onFinishReplacement = onFinishReplacement
        self.loadError = loadError
        self.launchProblems = launchProblems
        self.canDeleteActiveLibrary = canDeleteActiveLibrary
        _mode = State(initialValue: initialMode)
    }

    var body: some View {
        switch mode {
        case .choose:
            VStack(spacing: 0) {
                if let resumeError {
                    ErrorDetailDisclosure(error: resumeError)
                        .padding()
                }
                WelcomeChooseView(
                    loadError: loadError,
                    launchProblems: launchProblems,
                    canDeleteActiveLibrary: canDeleteActiveLibrary,
                    onLibraryReady: onLibraryReady,
                    onOpenLibrary: onOpenLibrary,
                    onFinishReplacement: onFinishReplacement,
                    onRestoreSaved: { mode = .restore($0) },
                    onJoin: { mode = .join },
                    onAddDevice: { mode = .restore(nil) }
                )
            }
            .task { await resumeUnfinishedJoin() }
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
        case .resume(let join):
            BootstrapCodeView(
                form: codeForm(.resume(join)),
                onBack: { mode = .choose }
            )
        }
    }

    /// Open the waiting screen of a join this device has under way, so bae
    /// goes on checking for its approval.
    private func resumeUnfinishedJoin() async {
        do {
            if let join = try await setup.unfinishedJoins().first {
                mode = .resume(join)
            }
        }
        catch is CancellationError {}
        catch {
            logger.error(
                "listing the joins under way failed: \(error.localizedDescription)"
            )
            resumeError = DisplayError(error)
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
    /// with the populated `LibrarySetup` and `LibraryRemovals` so no flow
    /// touches the real library directory or keychain.
    #Preview("Choose") {
        PreviewScenes.welcome(initialMode: .choose)
    }

    #Preview("Restore") {
        PreviewScenes.welcome(initialMode: .restore(nil))
    }

    #Preview("Join") {
        PreviewScenes.welcome(initialMode: .join)
    }
#endif
