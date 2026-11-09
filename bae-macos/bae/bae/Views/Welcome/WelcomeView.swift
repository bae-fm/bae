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
        /// A restore onto this device that stopped after it recorded itself,
        /// finished on the code screen.
        case resumeRestore(BridgeUnfinishedRestore)
    }

    @Environment(LibrarySetup.self)
    private var setup

    @State
    private var mode: Mode
    /// Listing the joins or restores under way failed; shown above the
    /// chooser.
    @State
    private var resumeError: DisplayError?
    /// Whether the chooser already went on with what was under way. It does
    /// so once, as it first shows: Back from a join or restore that failed
    /// again shows the chooser, and the next start goes on with it.
    @State
    private var resumed = false

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
                    ErrorNotice(error: resumeError)
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
            .task { await resumeUnfinished() }
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
        case .resumeRestore(let restore):
            BootstrapCodeView(
                form: codeForm(.resumeRestore(restore)),
                onBack: { mode = .choose }
            )
        }
    }

    /// Go on with what this device has under way, one at a time: a join's
    /// waiting screen, so bae goes on checking for its approval, or else a
    /// restore that stopped after it recorded itself, finished at once.
    private func resumeUnfinished() async {
        guard !resumed else { return }
        resumed = true
        do {
            if let join = try await setup.unfinishedJoins().first {
                mode = .resume(join)
            }
            else if let restore = try await setup.unfinishedRestores().first {
                mode = .resumeRestore(restore)
            }
        }
        catch is CancellationError {}
        catch {
            logger.error(
                "listing what is under way failed: \(String(reflecting: error))"
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
