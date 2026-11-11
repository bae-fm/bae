import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("WelcomeView")

/// First-run and add-a-library entry point. A thin switcher over the flows:
/// choose, the code sheets over it, and restore from a saved code, each with
/// its own state. A flow's `@State` starts fresh every time it appears, since
/// every flow is entered from the chooser, so nothing leaks between flows.
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
    /// callout, shown again when Back returns to the chooser from a flow a
    /// menu item opened directly.
    let loadError: LoadError?
    /// What launch found wrong, shown above `loadError` and never replaced by
    /// it.
    let launchProblems: LaunchProblems
    let canDeleteActiveLibrary: Bool

    enum Mode {
        case choose
        /// Restore from a code coven saved in iCloud Keychain.
        case restoreSaved(BridgeSavedRestoreCode)
        /// The Enter code sheet for a code the person types or pastes, or the
        /// scanner for one they scan, open as it appears when `scanning`.
        case code(scanning: Bool)
        /// A join this device had under way when bae stopped, continued on
        /// its waiting screen.
        case resume(BridgeUnfinishedJoin)
        /// A restore onto this device that stopped after it recorded itself,
        /// finished at once; if that fails, the Enter code sheet says why.
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
            chooser
                .task { await resumeUnfinished() }
        case .restoreSaved(let saved):
            BootstrapCodeView(
                form: codeForm(.restoreSaved(saved)),
                onBack: { mode = .choose }
            )
        case .code(let scanning):
            codeScreen(.code, scanning: scanning)
        case .resume(let join):
            codeScreen(.resume(join))
        case .resumeRestore(let restore):
            codeScreen(.resumeRestore(restore))
        }
    }

    private var chooser: some View {
        VStack(spacing: 0) {
            if let resumeError {
                Notice(error: resumeError)
                    .padding()
            }
            WelcomeChooseView(
                loadError: loadError,
                launchProblems: launchProblems,
                canDeleteActiveLibrary: canDeleteActiveLibrary,
                onLibraryReady: onLibraryReady,
                onOpenLibrary: onOpenLibrary,
                onFinishReplacement: onFinishReplacement,
                onRestoreSaved: { mode = .restoreSaved($0) },
                onScanCode: { mode = .code(scanning: true) },
                onEnterCode: { mode = .code(scanning: false) }
            )
        }
    }

    /// The code sheets over the chooser, for the flow `entry` opens.
    private func codeScreen(
        _ entry: BootstrapCodeForm.Entry,
        scanning: Bool = false
    ) -> some View {
        CodeEntryScreen(
            form: codeForm(entry),
            scanning: scanning,
            onClose: { mode = .choose },
            background: { chooser }
        )
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
        PreviewScenes.welcome(
            initialMode: .choose,
            setup: PreviewData.welcomeSetup()
        )
    }

    #Preview("Enter code") {
        PreviewScenes.welcome(
            initialMode: .code(scanning: false),
            setup: PreviewData.welcomeSetup()
        )
    }
#endif
