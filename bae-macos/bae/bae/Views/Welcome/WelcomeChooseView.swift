import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("WelcomeChooseView")

/// The first screen: open a library already on this device, restore one whose
/// restore code is in the keychain, or start one of the other flows (create,
/// join, restore-from-cloud). Owns the on-device discovery and the keychain
/// restore machinery the entry rows drive.
struct WelcomeChooseView: View {
    let onLibraryReady: (BridgeLibrary) -> Void
    let onJoin: () -> Void
    let onRestore: () -> Void

    @Environment(LibrarySetup.self)
    private var setup

    @State
    private var isCreating = false
    @State
    private var error: String?

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
    }

    private var content: some View {
        VStack(spacing: 32) {
            Spacer()
            Text(verbatim: "bae")
                .font(.system(size: 48, weight: .bold, design: .rounded))
            Text("Get started with your music library.")
                .font(.title3)
                .foregroundStyle(.secondary)
            if !localLibraries.isEmpty {
                LocalLibrariesSection(
                    libraries: localLibraries,
                    disabled: isCreating || isRestoring,
                    onOpen: onLibraryReady,
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
            VStack(spacing: 12) {
                if !hasExistingLibraries {
                    Button(action: doCreate) {
                        if isCreating {
                            ProgressView()
                                .controlSize(.small)
                        }
                        else {
                            Text("Create new library")
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(isCreating || isRestoring)
                    .keyboardShortcut(.defaultAction)
                }
                else {
                    Button(action: doCreate) {
                        if isCreating {
                            ProgressView()
                                .controlSize(.small)
                        }
                        else {
                            Text("Create new library")
                        }
                    }
                    .buttonStyle(.bordered)
                    .disabled(isCreating || isRestoring)
                }
                Button(action: onJoin) {
                    Text("Join a library")
                }
                .buttonStyle(.bordered)
                .disabled(isCreating || isRestoring)
                Button(action: onRestore) {
                    Text("Restore from cloud")
                }
                .buttonStyle(.bordered)
                .disabled(isCreating || isRestoring)
            }
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
            }
            Spacer()
        }
        .padding()
    }

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
                    self.error = error.displayLine
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
                self.error = error.displayLine
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
                        self.error = error.displayLine
                    }
                }
            }
        }
    #endif
}

#if DEBUG
    #Preview("First run") {
        WelcomeWindowChrome(loadError: nil) {
            WelcomeChooseView(
                onLibraryReady: { _ in },
                onJoin: {},
                onRestore: {},
            )
        }
        .environment(LibrarySetup.stub)
    }

    #Preview("Libraries and restore codes") {
        WelcomeWindowChrome(loadError: nil) {
            WelcomeChooseView(
                onLibraryReady: { _ in },
                onJoin: {},
                onRestore: {},
            )
        }
        .environment(PreviewData.welcomeSetup)
    }

    #Preview("Library failed to open") {
        WelcomeWindowChrome(
            loadError: "The library's settings could not be read."
        ) {
            WelcomeChooseView(
                onLibraryReady: { _ in },
                onJoin: {},
                onRestore: {},
            )
        }
        .environment(PreviewData.welcomeSetup)
    }
#endif
