import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("WelcomeChooseView")

/// The first screen: open a library already on this device, restore one whose
/// restore code is in the keychain, or start one of the other flows (create,
/// join, restore-from-cloud). Owns the on-device discovery and the keychain
/// restore machinery the entry rows drive.
struct WelcomeChooseView: View {
    /// A failed library open, surfaced inline as a callout under the subtitle.
    /// Prop-drilled from the app (the welcome window's chrome no longer carries
    /// it); nil when nothing failed.
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

    /// The in-flight keychain-row restore, owned so a superseding restore and
    /// the view's disappear can cancel it.
    @State
    private var restoreTask: Task<Void, Never>?
    @State
    private var isRestoring = false
    @State
    private var isAuthorizing = false
    @State
    private var oauthTokenJson: String?

    /// Libraries already on this device, discovered on appear. Listed first as
    /// the primary "open" path; reopening after a close lands here.
    @State
    private var localLibraries: [BridgeLibrary] = []

    /// iCloud Keychain restore
    @State
    private var keychainEntries: [(code: String, info: BridgeRestoreCodeInfo)] =
        []

    /// Keychain restore codes whose library isn't already on this device.
    /// On-device libraries open directly (the `localLibraries` section), so
    /// the restore section only offers the ones that need a cloud pull.
    private var restorableEntries: [(code: String, info: BridgeRestoreCodeInfo)]
    {
        keychainEntries.filter { entry in
            !localLibraries.contains { $0.id == entry.info.libraryId }
        }
    }

    /// Whether any library is already available to open or restore — drives
    /// whether "Create new library" is the prominent first-run action or one
    /// option among several.
    private var hasExistingLibraries: Bool {
        !localLibraries.isEmpty || !restorableEntries.isEmpty
    }

    var body: some View {
        // Scrolls only when the content outgrows the window (a populated
        // choose screen in a short window); the min-height keeps the Spacers
        // centering the content whenever it fits.
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
        VStack(spacing: 32) {
            Spacer()
            Text(verbatim: "bae")
                .font(.system(size: 48, weight: .bold, design: .rounded))
            Text("Get started with your music library.")
                .font(.title3)
                .foregroundStyle(.secondary)
            if let loadError {
                WelcomeLoadErrorCallout(error: loadError)
            }
            if !localLibraries.isEmpty {
                LocalLibrariesSection(
                    libraries: localLibraries,
                    disabled: isCreating || isRestoring
                        || removingLibraryId != nil,
                    canDeleteActiveLibrary: canDeleteActiveLibrary,
                    removingLibraryId: removingLibraryId,
                    onOpen: onLibraryReady,
                    onShowInFinder: { setup.revealInFinder($0.path) },
                    onRemove: { libraryPendingRemoval = $0 },
                )
            }
            if !restorableEntries.isEmpty {
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
            if hasExistingLibraries {
                populatedActions
            }
            else {
                firstRunActions
            }
            if let error {
                ErrorDetailDisclosure(error: error)
            }
            Spacer()
        }
        .padding()
    }

    /// The Create button's label: a spinner while a create is in flight, the
    /// title otherwise. Shared by both action layouts so the spinner behaves
    /// the same whether Create is the prominent first-run action or one small
    /// button among several.
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

    /// First run (no library on this device, nothing to restore): three stacked
    /// buttons at one width, Create the prominent default action.
    private var firstRunActions: some View {
        VStack(spacing: 12) {
            Button(action: doCreate) { createButtonLabel }
                .buttonStyle(.borderedProminent)
                .frame(width: 240)
                .disabled(isCreating || isRestoring)
                .keyboardShortcut(.defaultAction)
            Button("Join a library", action: onJoin)
                .buttonStyle(.bordered)
                .frame(width: 240)
                .disabled(isCreating || isRestoring)
            Button("Restore from cloud", action: onRestore)
                .buttonStyle(.bordered)
                .frame(width: 240)
                .disabled(isCreating || isRestoring)
        }
    }

    /// A library or restore entry already exists, so the three actions drop to
    /// a secondary horizontal row under a divider — Create is no longer the
    /// headline; opening or restoring an existing library is.
    private var populatedActions: some View {
        VStack(spacing: 12) {
            Divider()
                .frame(maxWidth: WelcomeLayout.columnWidth)
            HStack(spacing: 12) {
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
    /// Remove a keychain restore code (and its entry) after the section's
    /// confirmation. The section holds the confirmation dialog; the confirmed
    /// delete calls up here so the keychain write and the entry drop stay with
    /// the state's owner.
    private func deleteKeychainEntry(code: String) {
        guard let entry = keychainEntries.first(where: { $0.code == code })
        else {
            return
        }
        setup.deleteRestoreCode(entry.info.libraryId)
        keychainEntries.removeAll { $0.code == code }
    }

    private func removeLocalLibrary(_ library: BridgeLibrary) {
        removingLibraryId = library.id
        error = nil
        let remove = setup.removeLocalLibrary
        Task.detached {
            do {
                try remove(library.id)
                await MainActor.run {
                    localLibraries.removeAll { $0.id == library.id }
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
            localLibraries = discovered
        }
        catch is CancellationError {
        }
        catch {
            logger.warning(
                "Skipping local library discovery: \(error.localizedDescription)"
            )
            localLibraries = []
        }
    }

    private func checkKeychainForRestoreCodes() async {
        do {
            let fetch = setup.fetchRestoreCodes
            let decode = setup.decodeRestoreCode
            let decoded = try await DetachedWork.run {
                let stored = fetch()
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
            keychainEntries = decoded
        }
        catch is CancellationError {
        }
        catch {
            logger.warning(
                "Skipping keychain restore lookup: \(error.localizedDescription)"
            )
            keychainEntries = []
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

    /// Restore the keychain entry's library from its restore code. The bridge
    /// re-decodes the code, so the caller only passes the code plus whatever
    /// OAuth token a connect step already produced.
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
                // Superseded by a newer restore, which set `isRestoring = true`
                // for itself when it cancelled this one — leave the flag alone so
                // its spinner stays up. The superseding restore owns it.
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

/// The shared width of the populated choose screen's column. The libraries and
/// keychain sections, the divider, and the bottom actions all pin to it so the
/// screen reads as one column rather than a stack of differently-sized blocks.
enum WelcomeLayout {
    static let columnWidth: CGFloat = 400
}

/// The inline callout shown when a library failed to open: a warning glyph, a
/// bold title, the underlying message, and a line pointing at the ways forward.
/// A tinted rounded rect (native red opacities, not the mockup's hex colors)
/// sitting under the subtitle, column-width.
private struct WelcomeLoadErrorCallout: View {
    let error: DisplayError

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
            VStack(alignment: .leading, spacing: 4) {
                Text("Library failed to open")
                    .font(.headline)
                ErrorDetailDisclosure(error: error, showIcon: false)
                Text("Choose another library or restore from cloud.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .padding(12)
        .frame(maxWidth: WelcomeLayout.columnWidth, alignment: .leading)
        .background(Color.red.opacity(0.1))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(Color.red.opacity(0.3))
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
