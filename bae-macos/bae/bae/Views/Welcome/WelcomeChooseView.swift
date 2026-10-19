import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("WelcomeChooseView")

/// The first screen: open a library on this device, restore one from a code
/// saved in iCloud Keychain, or create, join, or restore by code.
struct WelcomeChooseView: View {
    /// A failed library open, shown as a callout under the subtitle.
    let loadError: DisplayError?
    let canDeleteActiveLibrary: Bool
    let onLibraryReady: (OpenedLibrary, BridgeError?) -> Void
    let onOpenLibrary: (String) -> Void
    /// Finish the reset or restore of a library's copy on this Mac that
    /// stopped partway, with the one call that finishes it.
    let onFinishReplacement: (String, BridgeReplacedBy) -> Void
    let onRestoreSaved: (BridgeSavedRestoreCode) -> Void
    let onJoin: () -> Void
    let onRestore: () -> Void

    @Environment(LibrarySetup.self)
    private var setup
    @Environment(LibraryRemovals.self)
    private var removals

    @State
    private var isCreating = false
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

    private var savedEntries: [SavedRestoreEntry] {
        listings.savedCodes.value ?? []
    }

    /// Lead with "Create new library" only when both lookups finished and found
    /// nothing; a loading or failed lookup doesn't mean there are no libraries.
    private var isFirstRun: Bool {
        listings.localLibraries.value?.isEmpty == true
            && listings.savedCodes.value?.isEmpty == true
    }

    private var isBusy: Bool {
        isCreating || removals.removingLibraryId != nil || removals.isRetrying
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
            "Remove this library from this Mac?",
            isPresented: Binding(
                get: { pendingRemoval != nil },
                set: { if !$0 { pendingRemoval = nil } }
            ),
            presenting: pendingRemoval
        ) { pending in
            Button("Remove", role: .destructive) {
                Task {
                    await listings.remove(
                        closedLibrary: pending.library.id,
                        with: removals,
                        from: setup
                    )
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: { pending in
            Text(
                pending.unsent.confirmation(
                    consequence: LibraryRemovalConfirmation.consequence,
                    message: LibraryRemovalConfirmation.message(
                        for: pending.library
                    )
                )
            )
        }
    }

    /// Read what removing `library` would lose, then ask. A failure shows as
    /// the removal's error.
    private func askToRemove(_ library: BridgeLibrary) {
        Task {
            if let unsent = await removals.unsentWork(closedLibrary: library.id)
            {
                pendingRemoval = PendingRemoval(
                    library: library,
                    unsent: unsent
                )
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
            if let loadError {
                WelcomeLoadErrorCallout(
                    title: "Library failed to open",
                    error: loadError,
                    guidance: "Choose another library or restore from cloud."
                )
            }
            if let removalError = removals.error {
                RemovalErrorCallout(error: removalError, removals: removals)
            }
            if let failure = listings.localLibraries.failure {
                WelcomeLoadErrorCallout(
                    title: "Couldn't list the libraries on this Mac",
                    error: failure
                )
            }
            else if !discoveredLibraries.isEmpty {
                LocalLibrariesSection(
                    libraries: discoveredLibraries,
                    disabled: isBusy,
                    canDeleteActiveLibrary: canDeleteActiveLibrary,
                    removingLibraryId: removals.removingLibraryId,
                    onOpen: { onOpenLibrary($0.id) },
                    onFinish: { onFinishReplacement($0.id, $1) },
                    onRemove: { askToRemove($0) }
                )
            }
            if let failure = listings.savedCodes.failure {
                WelcomeLoadErrorCallout(
                    title: "Couldn't read restore codes from your keychain",
                    error: failure
                )
            }
            else if !savedEntries.isEmpty {
                KeychainRestoreSection(
                    entries: savedEntries,
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
            if let error {
                ErrorDetailDisclosure(error: error)
            }
            Spacer()
        }
        .padding()
    }

    /// The Create button's label, a spinner while a create runs.
    @ViewBuilder
    private var createButtonLabel: some View {
        if isCreating {
            ProgressView()
                .controlSize(.small)
        }
        else {
            Text("Create new library")
        }
    }

    /// The width the first-run buttons share.
    private static let firstRunButtonWidth: CGFloat = 240

    /// First run: three stacked buttons with Create as the default action.
    private var firstRunActions: some View {
        VStack(spacing: ThemeSpace.group) {
            Button(action: doCreate) { createButtonLabel }
                .buttonStyle(PrimaryButtonStyle())
                .frame(width: Self.firstRunButtonWidth)
                .disabled(isBusy)
                .keyboardShortcut(.defaultAction)
            Button("Join a library", action: onJoin)
                .buttonStyle(.bordered)
                .frame(width: Self.firstRunButtonWidth)
                .disabled(isBusy)
            Button("Restore from cloud", action: onRestore)
                .buttonStyle(.bordered)
                .frame(width: Self.firstRunButtonWidth)
                .disabled(isBusy)
        }
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
                Button("Join a library", action: onJoin)
                    .disabled(isBusy)
                Button("Restore from cloud", action: onRestore)
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
        createTask = Task {
            defer { isCreating = false }
            do {
                let library = try await create()
                onLibraryReady(library, nil)
            }
            catch {
                logger.error(
                    "Library creation failed: \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
        }
    }
}

/// The choose screen's shared column width.
enum WelcomeLayout {
    static let columnWidth: CGFloat = 400
}

/// A danger notice for a failed open or lookup: a title naming what failed,
/// the error, and optional guidance.
private struct WelcomeLoadErrorCallout: View {
    let title: LocalizedStringKey
    let error: DisplayError
    /// What the user can do next, if the screen has a suggestion.
    var guidance: LocalizedStringKey?

    var body: some View {
        HStack(alignment: .top, spacing: ThemeSpace.related) {
            if let symbol = StatusTone.danger.symbol {
                Image(systemName: symbol)
                    .foregroundStyle(StatusTone.danger.color)
            }
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(title)
                    .themeText(.heading)
                ErrorDetailDisclosure(error: error, showIcon: false)
                if let guidance {
                    Text(guidance)
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
        }
        .notice(.danger)
        .frame(maxWidth: WelcomeLayout.columnWidth, alignment: .leading)
    }
}

/// A library the person is asked to remove, with what removing it loses.
private struct PendingRemoval {
    let library: BridgeLibrary
    let unsent: BridgeUnsentWork
}

/// A removal that failed, or left this Mac's files for a library it already
/// left, with Retry for the latter.
private struct RemovalErrorCallout: View {
    let error: DisplayError
    let removals: LibraryRemovals

    var body: some View {
        HStack(alignment: .top, spacing: ThemeSpace.related) {
            if let symbol = StatusTone.danger.symbol {
                Image(systemName: symbol)
                    .foregroundStyle(StatusTone.danger.color)
            }
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text("Couldn't remove the library")
                    .themeText(.heading)
                ErrorDetailDisclosure(error: error, showIcon: false)
                if removals.unfinished != nil {
                    Button("Retry") { removals.retryUnfinished() }
                        .disabled(removals.isRetrying)
                }
            }
            Spacer(minLength: 0)
        }
        .notice(.danger)
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
                onJoin: {},
                onRestore: {}
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
                onJoin: {},
                onRestore: {}
            )
        }
        .environment(PreviewData.welcomeSetup())
        .environment(PreviewData.welcomeRemovals())
    }

    #Preview("Library failed to open") {
        WelcomeWindowChrome {
            WelcomeChooseView(
                loadError: PreviewData.displayErrorWithDetail,
                canDeleteActiveLibrary: true,
                onLibraryReady: { _, _ in },
                onOpenLibrary: { _ in },
                onFinishReplacement: { _, _ in },
                onRestoreSaved: { _ in },
                onJoin: {},
                onRestore: {}
            )
        }
        .environment(PreviewData.welcomeSetup())
        .environment(PreviewData.welcomeRemovals())
    }
#endif
