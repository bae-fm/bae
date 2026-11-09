import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("WelcomeChooseView")

/// The first screen: open a library on this device, restore one from a code
/// saved in iCloud Keychain, create one, join someone's library with an
/// invite, or add this Mac to one of the person's own libraries with its code.
struct WelcomeChooseView: View {
    /// A failed library open or dev reset, shown as a callout under the
    /// subtitle, titled by the step that failed.
    let loadError: LoadError?
    /// What launch found wrong, shown above `loadError` and never replaced by
    /// it.
    var launchProblems = LaunchProblems.none
    let canDeleteActiveLibrary: Bool
    let onLibraryReady: (OpenedLibrary, BridgeError?) -> Void
    let onOpenLibrary: (String) -> Void
    /// Finish the reset or restore of a library's copy on this Mac that
    /// stopped partway, with the one call that finishes it.
    let onFinishReplacement: (String, BridgeReplacedBy) -> Void
    let onRestoreSaved: (BridgeSavedRestoreCode) -> Void
    let onScanCode: () -> Void
    let onEnterCode: () -> Void

    @Environment(LibrarySetup.self)
    private var setup
    @Environment(LibraryRemovals.self)
    private var removals

    @State
    private var isCreating = false
    /// The running create's steps, which show under the buttons.
    @State
    private var createSteps: LibrarySetupSteps?
    @State
    private var createTask: Task<Void, Never>?
    @State
    private var error: DisplayError?
    /// The library the person is asked to remove, with what removing it
    /// would lose.
    @State
    private var pendingRemoval: PendingRemoval?

    /// Libraries on this Mac and the restore codes saved for others, read on
    /// appear.
    @State
    private var listings = WelcomeListings()

    private var discoveredLibraries: [BridgeLibrary] {
        listings.localLibraries.value ?? []
    }

    /// Lead with "Create new library" only when both lookups finished and found
    /// nothing; a loading or failed lookup doesn't mean there are no libraries.
    private var isFirstRun: Bool {
        listings.localLibraries.value?.isEmpty == true
            && listings.savedCodes.list.value?.isEmpty == true
    }

    private var isBusy: Bool {
        isCreating || removals.removing != nil || removals.isRetrying
    }

    var body: some View {
        // Scrolls only when the content outgrows the window; the min-height
        // keeps it centered otherwise.
        GeometryReader { geometry in
            ScrollView {
                content
                    .frame(
                        maxWidth: .infinity,
                        minHeight: geometry.size.height
                    )
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .task { await listings.load(from: setup) }
        .onDisappear { createTask?.cancel() }
        .alert(
            pendingRemoval.map {
                LibraryRemovalConfirmation.Ending(of: $0.library).title
            } ?? "",
            isPresented: Binding(
                get: { pendingRemoval != nil },
                set: { if !$0 { pendingRemoval = nil } }
            ),
            presenting: pendingRemoval
        ) { pending in
            Button(
                LibraryRemovalConfirmation.Ending(of: pending.library).action,
                role: .destructive
            ) {
                Task {
                    await listings.remove(
                        closedLibrary: pending.library.id,
                        losing: pending.loss,
                        with: removals,
                        from: setup
                    )
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: { pending in
            Text(
                LibraryRemovalConfirmation.text(
                    for: pending.library,
                    losing: pending.loss
                )
            )
        }
    }

    /// Read what removing `library` would lose, then ask. A failure shows as
    /// the removal's error.
    private func askToRemove(_ library: BridgeLibrary) {
        Task {
            if let loss = await removals.loss(closedLibrary: library) {
                pendingRemoval = PendingRemoval(library: library, loss: loss)
            }
        }
    }

    private var content: some View {
        VStack(spacing: ThemeSpace.page) {
            Spacer()
            Text(verbatim: "bae")
                .themeText(.wordmark)
            Text("Get started with your music library.")
                .themeText(.heading)
                .foregroundStyle(.secondary)
            LaunchProblemNotices(launchProblems)
                .frame(maxWidth: WelcomeLayout.columnWidth)
            if let loadError {
                ErrorNotice(
                    title: loadError.step.title,
                    advice: loadError.step.advice,
                    error: loadError.error
                )
                .frame(maxWidth: WelcomeLayout.columnWidth)
            }
            if let removalError = removals.error {
                RemovalErrorCallout(error: removalError, removals: removals)
            }
            if let failure = listings.localLibraries.failure {
                ErrorNotice(
                    title: String(
                        localized: "Couldn't list the libraries on this Mac"
                    ),
                    advice: nil,
                    error: failure
                )
                .frame(maxWidth: WelcomeLayout.columnWidth)
            }
            else if !discoveredLibraries.isEmpty {
                LocalLibrariesSection(
                    libraries: discoveredLibraries,
                    disabled: isBusy,
                    canDeleteActiveLibrary: canDeleteActiveLibrary,
                    removing: removals.removing,
                    opening: removals.opening,
                    onOpen: { onOpenLibrary($0.id) },
                    onFinish: { onFinishReplacement($0.id, $1) },
                    onFinishRemoving: { library in
                        Task {
                            await listings.finishRemoving(
                                library.id,
                                with: removals,
                                from: setup
                            )
                        }
                    },
                    onRemove: { askToRemove($0) }
                )
            }
            if let failure = listings.savedCodes.list.failure {
                ErrorNotice(
                    title: String(
                        localized:
                            "Couldn't read restore codes from your keychain"
                    ),
                    advice: nil,
                    error: failure
                )
                .frame(maxWidth: WelcomeLayout.columnWidth)
            }
            else if listings.savedCodes.list.value?.isEmpty == false {
                if let deleteError = listings.savedCodes.deleteError {
                    ErrorNotice(
                        title: String(
                            localized: "Couldn't delete the restore code"
                        ),
                        advice: nil,
                        error: deleteError
                    )
                    .frame(maxWidth: WelcomeLayout.columnWidth)
                }
                KeychainRestoreSection(
                    codes: listings.savedCodes,
                    disabled: isBusy,
                    onRestore: { onRestoreSaved($0.code) }
                )
            }
            if isFirstRun {
                firstRunActions
            }
            else {
                populatedActions
            }
            if let createSteps {
                StepProgressLine(steps: createSteps.steps) { EmptyView() }
                    .frame(maxWidth: StepProgressLayout.width)
            }
            if let error {
                ErrorNotice(error: error)
            }
            Spacer()
        }
        .padding()
    }

    /// The Create button's label.
    private var createButtonLabel: some View {
        Text("Create new library")
    }

    /// The width the first-run buttons share.
    private static let firstRunButtonWidth: CGFloat = 240

    /// First run: Create as the default action, then Scan code and Enter
    /// code side by side.
    private var firstRunActions: some View {
        VStack(spacing: ThemeSpace.related + ThemeSpace.line) {
            Button(action: doCreate) { createButtonLabel }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(isBusy)
                .keyboardShortcut(.defaultAction)
            HStack(spacing: ThemeSpace.related) {
                Button("Scan code", action: onScanCode)
                    .frame(maxWidth: .infinity)
                Button("Enter code", action: onEnterCode)
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .controlSize(.large)
            .disabled(isBusy)
        }
        .frame(width: Self.firstRunButtonWidth)
    }

    /// With a library or restore code present, the actions become a row of
    /// smaller buttons under a divider.
    private var populatedActions: some View {
        VStack(spacing: ThemeSpace.group) {
            Divider()
                .frame(maxWidth: WelcomeLayout.columnWidth)
            HStack(spacing: ThemeSpace.group) {
                Button(action: doCreate) { createButtonLabel }
                    .disabled(isBusy)
                Button("Scan code", action: onScanCode)
                    .disabled(isBusy)
                Button("Enter code", action: onEnterCode)
                    .disabled(isBusy)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
    }
}

// MARK: - Actions

extension WelcomeChooseView {
    private func doCreate() {
        createTask?.cancel()
        isCreating = true
        error = nil
        let create = setup.createLibrary
        let steps = setup.setupSteps()
        createSteps = steps
        createTask = Task {
            defer {
                isCreating = false
                createSteps = nil
            }
            do {
                let library = try await create(steps.setup)
                onLibraryReady(library, nil)
            }
            catch {
                logger.error(
                    "Library creation failed: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }
}

/// The choose screen's shared column width.
enum WelcomeLayout {
    static let columnWidth: CGFloat = 420
}

/// A library the person is asked to remove, with what removing it loses.
private struct PendingRemoval {
    let library: BridgeLibrary
    let loss: RemovalLoss
}

/// A removal that failed, or left this Mac's files for a library it already
/// left, with Retry for the latter.
private struct RemovalErrorCallout: View {
    let error: DisplayError
    let removals: LibraryRemovals

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            ErrorNotice(
                title: String(localized: "Couldn't remove the library"),
                advice: nil,
                error: error,
                action: removals.unfinished == nil
                    ? nil
                    : ErrorAction(isDisabled: removals.isRetrying) {
                        removals.retryUnfinished()
                    }
            )
            if let finishing = removals.finishing {
                StepProgressLine(steps: finishing.steps) {
                    EmptyView()
                }
            }
        }
        .frame(maxWidth: WelcomeLayout.columnWidth, alignment: .leading)
    }
}

#if DEBUG
    #Preview("First run") {
        WelcomeWindowChrome {
            WelcomeChooseView(
                loadError: nil,
                canDeleteActiveLibrary: true,
                onLibraryReady: { _, _ in },
                onOpenLibrary: { _ in },
                onFinishReplacement: { _, _ in },
                onRestoreSaved: { _ in },
                onScanCode: {},
                onEnterCode: {}
            )
        }
        .environment(LibrarySetup.stub())
        .environment(PreviewData.welcomeRemovals())
    }

    #Preview("Libraries") {
        WelcomeWindowChrome {
            WelcomeChooseView(
                loadError: nil,
                canDeleteActiveLibrary: true,
                onLibraryReady: { _, _ in },
                onOpenLibrary: { _ in },
                onFinishReplacement: { _, _ in },
                onRestoreSaved: { _ in },
                onScanCode: {},
                onEnterCode: {}
            )
        }
        .environment(PreviewData.welcomeSetup())
        .environment(PreviewData.welcomeRemovals())
    }

    #Preview("Library failed to open") {
        WelcomeWindowChrome {
            WelcomeChooseView(
                loadError: LoadError(
                    .libraryOpen,
                    PreviewData.displayErrorWithDetail
                ),
                canDeleteActiveLibrary: true,
                onLibraryReady: { _, _ in },
                onOpenLibrary: { _ in },
                onFinishReplacement: { _, _ in },
                onRestoreSaved: { _ in },
                onScanCode: {},
                onEnterCode: {}
            )
        }
        .environment(PreviewData.welcomeSetup())
        .environment(PreviewData.welcomeRemovals())
    }

    #if BAE_DEV_RESET
        #Preview("Dev reset failed") {
            WelcomeWindowChrome {
                WelcomeChooseView(
                    loadError: LoadError(
                        .devReset,
                        BridgeError.Diagnostic(
                            category: .config,
                            detail:
                                "dev profile profile.toml: line 11: unknown field `key_prefix`"
                        )
                    ),
                    canDeleteActiveLibrary: true,
                    onLibraryReady: { _, _ in },
                    onOpenLibrary: { _ in },
                    onFinishReplacement: { _, _ in },
                    onRestoreSaved: { _ in },
                    onScanCode: {},
                    onEnterCode: {}
                )
            }
            .environment(PreviewData.welcomeSetup())
            .environment(PreviewData.welcomeRemovals())
        }
    #endif
#endif
