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
    private var localLibraries: SectionLoad<[BridgeLibrary]> = .loading

    /// iCloud Keychain restore
    @State
    private var keychainEntries:
        SectionLoad<[(code: String, info: BridgeRestoreCodeInfo)]> = .loading

    private var discoveredLibraries: [BridgeLibrary] {
        localLibraries.value ?? []
    }

    /// Keychain restore codes whose library isn't already on this device.
    /// On-device libraries open directly (the `localLibraries` section), so
    /// the restore section only offers the ones that need a cloud pull.
    private var restorableEntries: [(code: String, info: BridgeRestoreCodeInfo)]
    {
        (keychainEntries.value ?? [])
            .filter { entry in
                !discoveredLibraries.contains { $0.id == entry.info.libraryId }
            }
    }

    /// Whether to lead with the prominent "Create new library" layout instead of
    /// the secondary action row. Only when both lookups finished and found
    /// nothing: a section still loading, or one that failed, is not a device
    /// with no libraries on it, and offering the first-run wall on either is how
    /// a locked keychain or an unreadable libraries directory came to read as a
    /// brand new install.
    private var isFirstRun: Bool {
        localLibraries.value?.isEmpty == true
            && keychainEntries.value?.isEmpty == true
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
        guard let entries = keychainEntries.value,
            let entry = entries.first(where: { $0.code == code })
        else {
            return
        }
        setup.deleteRestoreCode(entry.info.libraryId)
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
            // Not an empty list: a discovery that never ran cannot say the
            // device has no libraries, and saying it puts a first-run wall in
            // front of someone whose libraries are right there on disk.
            //
            // A failure core says has no line to show is a cancellation — the
            // same case the arm above catches for Swift's own, where the view
            // is going away and this section's state stops mattering.
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
            // The keychain refusing the lookup (locked, or the display asleep)
            // is not the same answer as holding no restore codes, and the user
            // is the one who can tell them apart. A failure with no line to
            // show is a cancellation, handled as above.
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

/// What one of the welcome screen's lookups knows so far. A failure is its own
/// case rather than an empty result: "there is nothing here" and "we could not
/// look" are different screens, and collapsing them is what turned a locked
/// keychain into a first-run wall.
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

/// The shared width of the populated choose screen's column. The libraries and
/// keychain sections, the divider, and the bottom actions all pin to it so the
/// screen reads as one column rather than a stack of differently-sized blocks.
enum WelcomeLayout {
    static let columnWidth: CGFloat = 400
}

/// The inline callout for something the welcome screen could not do: a warning
/// glyph, a bold title naming what failed, the underlying message, and — when
/// there is one — a line pointing at the ways forward. A tinted rounded rect
/// (native red opacities, not the mockup's hex colors), column-width.
///
/// Used for the failed open and for either section lookup coming back broken.
/// A section that failed shows this in its own place rather than rendering as
/// an empty list, which is a different and wrong claim.
private struct WelcomeLoadErrorCallout: View {
    let title: LocalizedStringKey
    let error: DisplayError
    /// What the user can do next, when the screen has something to suggest. A
    /// lookup that broke does not — the actions below are all still there.
    var guidance: LocalizedStringKey?

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.headline)
                ErrorDetailDisclosure(error: error, showIcon: false)
                if let guidance {
                    Text(guidance)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
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
