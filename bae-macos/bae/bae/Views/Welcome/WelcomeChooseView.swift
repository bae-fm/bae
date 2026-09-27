import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("WelcomeChooseView")

/// The first screen: open a library on this device, restore one from the
/// keychain, or create, join, or restore from the cloud.
struct WelcomeChooseView: View {
    /// A failed library open, shown as a callout under the subtitle.
    let loadError: DisplayError?
    let canDeleteActiveLibrary: Bool
    let onLibraryReady: (BridgeLibrary) -> Void
    let onJoin: () -> Void
    let onRestore: () -> Void

    @Environment(LibrarySetup.self)
    private var setup

    @State
    private var isCreating = false
    @State
    private var error: DisplayError?

    @State
    private var libraryPendingRemoval: BridgeLibrary?
    @State
    private var removingLibraryId: String?

    /// The in-flight keychain restore, cancelled by a newer one or on
    /// disappear.
    @State
    private var restoreTask: Task<Void, Never>?
    @State
    private var isRestoring = false
    @State
    private var isAuthorizing = false
    @State
    private var oauthTokenJson: String?

    /// Libraries on this device, discovered on appear.
    @State
    private var localLibraries: SectionLoad<[BridgeLibrary]> = .loading

    /// Restore codes found in the keychain.
    @State
    private var keychainEntries:
        SectionLoad<[(code: String, info: BridgeRestoreCodeInfo)]> = .loading

    private var discoveredLibraries: [BridgeLibrary] {
        localLibraries.value ?? []
    }

    /// Keychain restore codes whose library isn't already on this device.
    private var restorableEntries: [(code: String, info: BridgeRestoreCodeInfo)]
    {
        (keychainEntries.value ?? [])
            .filter { entry in
                !discoveredLibraries.contains { $0.id == entry.info.libraryId }
            }
    }

    /// Lead with "Create new library" only when both lookups finished and found
    /// nothing; a loading or failed lookup doesn't mean there are no libraries.
    private var isFirstRun: Bool {
        localLibraries.value?.isEmpty == true
            && keychainEntries.value?.isEmpty == true
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
        .task {
            await loadLocalLibraries()
        }
        .task {
            await checkKeychainForRestoreCodes()
        }
        .onDisappear { restoreTask?.cancel() }
        .alert(
            "Remove this library from this Mac?",
            isPresented: Binding(
                get: { libraryPendingRemoval != nil },
                set: { if !$0 { libraryPendingRemoval = nil } }
            ),
            presenting: libraryPendingRemoval
        ) { library in
            Button("Delete", role: .destructive) {
                removeLocalLibrary(library)
            }
            Button("Cancel", role: .cancel) {}
        } message: { library in
            Text(LibraryRemovalConfirmation.message(for: library))
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
            if let failure = localLibraries.failure {
                WelcomeLoadErrorCallout(
                    title: "Couldn't list the libraries on this Mac",
                    error: failure
                )
            }
            else if !discoveredLibraries.isEmpty {
                LocalLibrariesSection(
                    libraries: discoveredLibraries,
                    disabled: isCreating || isRestoring
                        || removingLibraryId != nil,
                    canDeleteActiveLibrary: canDeleteActiveLibrary,
                    removingLibraryId: removingLibraryId,
                    onOpen: onLibraryReady,
                    onShowInFinder: { setup.revealInFinder($0.path) },
                    onRemove: { libraryPendingRemoval = $0 },
                )
            }
            if let failure = keychainEntries.failure {
                WelcomeLoadErrorCallout(
                    title: "Couldn't read restore codes from your keychain",
                    error: failure
                )
            }
            else if !restorableEntries.isEmpty {
                KeychainRestoreSection(
                    entries: restorableEntries,
                    isRestoring: isRestoring,
                    isAuthorizing: isAuthorizing,
                    oauthConnected: oauthTokenJson != nil,
                    onRestore: { entry in doRestoreFromCode(code: entry.code) },
                    onConnect: { info in
                        #if BAE_OAUTH_PROVIDERS
                            doOAuthAuthorize(provider: info.cloudProvider)
                        #endif
                    },
                    onCancelAuth: {
                        #if BAE_OAUTH_PROVIDERS
                            setup.oauthCancel()
                        #endif
                        isAuthorizing = false
                    },
                    onDelete: deleteKeychainEntry,
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
                .disabled(isCreating || isRestoring)
                .keyboardShortcut(.defaultAction)
            Button("Join a library", action: onJoin)
                .buttonStyle(.bordered)
                .frame(width: Self.firstRunButtonWidth)
                .disabled(isCreating || isRestoring)
            Button("Restore from cloud", action: onRestore)
                .buttonStyle(.bordered)
                .frame(width: Self.firstRunButtonWidth)
                .disabled(isCreating || isRestoring)
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
                    .disabled(isCreating || isRestoring)
                Button("Join a library", action: onJoin)
                    .disabled(isCreating || isRestoring)
                Button("Restore from cloud", action: onRestore)
                    .disabled(isCreating || isRestoring)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
    }

}

// MARK: - Actions

extension WelcomeChooseView {
    /// Removes a keychain restore code after the section's confirmation.
    private func deleteKeychainEntry(code: String) {
        guard let entries = keychainEntries.value,
            let entry = entries.first(where: { $0.code == code })
        else {
            return
        }
        do {
            try setup.deleteRestoreCode(entry.info.libraryId)
        }
        catch {
            // Keep the row: the code is still in the keychain.
            logger.error(
                "Failed to delete keychain restore code: \(error.localizedDescription)"
            )
            self.error = DisplayError(error)
            return
        }
        keychainEntries = .loaded(entries.filter { $0.code != code })
    }

    private func removeLocalLibrary(_ library: BridgeLibrary) {
        removingLibraryId = library.id
        error = nil
        let remove = setup.removeLocalLibrary
        Task.detached {
            do {
                try remove(library.id)
                await MainActor.run {
                    localLibraries = .loaded(
                        discoveredLibraries.filter { $0.id != library.id }
                    )
                    removingLibraryId = nil
                }
            }
            catch {
                await MainActor.run {
                    removingLibraryId = nil
                    self.error = DisplayError(error)
                }
            }
        }
    }

    private func loadLocalLibraries() async {
        do {
            let discover = setup.discoverLibraries
            let discovered = try await DetachedWork.run {
                try discover()
            }
            try Task.checkCancellation()
            localLibraries = .loaded(discovered)
        }
        catch is CancellationError {
        }
        catch {
            // A failure, not an empty list, so it doesn't read as first run.
            // No line means core reported a cancellation.
            guard let failure = DisplayError(error) else {
                logger.debug("Local library discovery cancelled")
                return
            }
            logger.error(
                "Local library discovery failed: \(error.localizedDescription)"
            )
            localLibraries = .failed(failure)
        }
    }

    private func checkKeychainForRestoreCodes() async {
        do {
            let fetch = setup.fetchRestoreCodes
            let decode = setup.decodeRestoreCode
            let decoded = try await DetachedWork.run {
                let stored = try fetch()
                var decoded: [(code: String, info: BridgeRestoreCodeInfo)] =
                    []
                for entry in stored {
                    do {
                        let info = try decode(entry.code)
                        decoded.append((code: entry.code, info: info))
                    }
                    catch {
                        logger.warning(
                            "Skipping unreadable keychain restore entry: \(error.localizedDescription)"
                        )
                    }
                }
                return decoded
            }
            try Task.checkCancellation()
            keychainEntries = .loaded(decoded)
        }
        catch is CancellationError {
        }
        catch {
            // A refused lookup (such as a locked keychain) isn't the same as
            // no restore codes. No line means core reported a cancellation.
            guard let failure = DisplayError(error) else {
                logger.debug("Keychain restore lookup cancelled")
                return
            }
            logger.error(
                "Keychain restore lookup failed: \(error.localizedDescription)"
            )
            keychainEntries = .failed(failure)
        }
    }

    private func doCreate() {
        isCreating = true
        error = nil
        let create = setup.createLibrary
        Task.detached {
            do {
                let info = try create()
                await MainActor.run {
                    isCreating = false
                    onLibraryReady(info)
                }
            }
            catch {
                await MainActor.run {
                    isCreating = false
                    self.error = DisplayError(error)
                }
            }
        }
    }

    /// Restores a keychain entry's library from its code and any OAuth token.
    private func doRestoreFromCode(code: String) {
        let token = oauthTokenJson
        let restore = setup.restoreFromCode
        restoreTask?.cancel()
        isRestoring = true
        error = nil
        restoreTask = Task {
            do {
                let restored = try await DetachedWork.run {
                    try restore(code, token)
                }
                try Task.checkCancellation()
                isRestoring = false
                onLibraryReady(restored)
            }
            catch is CancellationError {
                // The newer restore owns `isRestoring` now.
                logger.debug("Restore superseded by a newer restore; skipping")
            }
            catch {
                isRestoring = false
                self.error = DisplayError(error)
            }
        }
    }

    #if BAE_OAUTH_PROVIDERS
        private func doOAuthAuthorize(provider: BridgeCloudProvider) {
            isAuthorizing = true
            error = nil
            let authorize = setup.oauthAuthorize
            Task.detached {
                do {
                    let tokenJson = try authorize(provider)
                    await MainActor.run {
                        guard isAuthorizing else {
                            return
                        }
                        isAuthorizing = false
                        oauthTokenJson = tokenJson
                    }
                }
                catch {
                    await MainActor.run {
                        isAuthorizing = false
                        self.error = DisplayError(error)
                    }
                }
            }
        }
    #endif
}

/// A welcome-screen lookup's state; a failure is kept apart from an empty
/// result because they show different screens.
private enum SectionLoad<Value> {
    case loading
    case loaded(Value)
    case failed(DisplayError)

    var value: Value? {
        if case .loaded(let value) = self { return value }
        return nil
    }

    var failure: DisplayError? {
        if case .failed(let failure) = self { return failure }
        return nil
    }
}

/// The choose screen's shared column width.
enum WelcomeLayout {
    static let columnWidth: CGFloat = 400
}

/// An error callout for a failed open or lookup: a title naming what failed,
/// the error, and optional guidance, on the error notice background.
private struct WelcomeLoadErrorCallout: View {
    let title: LocalizedStringKey
    let error: DisplayError
    /// What the user can do next, if the screen has a suggestion.
    var guidance: LocalizedStringKey?

    var body: some View {
        HStack(alignment: .top, spacing: ThemeSpace.related) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(NoticeTone.error.tint)
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
        .padding(ThemeSpace.group)
        .frame(maxWidth: WelcomeLayout.columnWidth, alignment: .leading)
        .noticeBackground(.error)
        .overlay(
            RoundedRectangle(cornerRadius: ThemeRadius.control)
                .strokeBorder(
                    NoticeTone.error.tint.opacity(ThemeOpacity.tintStrong)
                )
        )
    }
}

#if DEBUG
    #Preview("First run") {
        WelcomeWindowChrome {
            WelcomeChooseView(
                loadError: nil,
                canDeleteActiveLibrary: true,
                onLibraryReady: { _ in },
                onJoin: {},
                onRestore: {},
            )
        }
        .environment(LibrarySetup.stub())
    }

    #Preview("Libraries and restore codes") {
        WelcomeWindowChrome {
            WelcomeChooseView(
                loadError: nil,
                canDeleteActiveLibrary: true,
                onLibraryReady: { _ in },
                onJoin: {},
                onRestore: {},
            )
        }
        .environment(PreviewData.welcomeSetup())
    }

    #Preview("Library failed to open") {
        WelcomeWindowChrome {
            WelcomeChooseView(
                loadError: PreviewData.displayErrorWithDetail,
                canDeleteActiveLibrary: true,
                onLibraryReady: { _ in },
                onJoin: {},
                onRestore: {},
            )
        }
        .environment(PreviewData.welcomeSetup())
    }
#endif
