import SwiftUI
import os.log

private let logger = Logger.bae("LibrarySettings")

struct LibrarySettingsTab: View {
    /// Remove the active library from this device. Implemented by AppDelegate.
    let onForgetLibrary: () -> Void

    @Environment(Sync.self)
    var sync
    @Environment(ConfigStore.self)
    var configStore
    @Environment(UiStore.self)
    var uiStore
    @Environment(OutboxStore.self)
    var outboxStore

    @State
    private var error: String?
    @State
    private var showSyncSetup = false
    @State
    private var showDisconnectConfirm = false
    @State
    private var showForgetConfirm = false
    /// Captured at the moment the user clicks Disconnect: bae-core's
    /// pre-formatted warning when releases live only in the cloud (`nil`
    /// when no releases are at risk).
    @State
    private var disconnectExtraWarning: String?
    @State
    private var disconnectWarningTask: Task<Void, Never>?

    private var isConnected: Bool {
        configStore.syncReady
    }

    var body: some View {
        Form {
            Section {
                LabeledContent("Path") {
                    HStack {
                        Text(configStore.config.libraryPath)
                            .lineLimit(1)
                            .truncationMode(.middle)
                            .foregroundStyle(.secondary)
                        Button {
                            SystemActions.revealInFinder(
                                path: configStore.config.libraryPath
                            )
                        } label: {
                            Image(systemName: "folder")
                        }
                        .buttonStyle(.borderless)
                        .help("Reveal in Finder")
                    }
                }
            }

            Section("Sync") {
                SyncErrorBanner(onReconnect: { showSyncSetup = true })

                if let sync = configStore.config.sync {
                    CloudProviderConnectedSection(
                        config: sync,
                        onDisconnect: { promptDisconnect() },
                    )
                }
                else {
                    Button("Set up sync...") {
                        showSyncSetup = true
                    }
                }
            }

            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
            }

            if isConnected {
                MembersSection()
                RecoveryCodeSection(generate: sync.generateRestoreCode)
            }

            Section("Remove") {
                Text(removeFooter)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button(
                    "Remove this library from this Mac...",
                    role: .destructive
                ) {
                    showForgetConfirm = true
                }
            }
        }
        .formStyle(.grouped)
        .sheet(isPresented: $showSyncSetup) {
            SyncSetupWizard(
                onConnectS3: { config in
                    try await sync.saveSyncConfig(config)
                    storeRestoreCode()
                },
                onConnectOAuth: { provider, storage in
                    try await sync.signInCloudProvider(provider, storage)
                    storeRestoreCode()
                },
                onConnectCloudKit: { storage in
                    try await sync.connectCloudkit(storage)
                    storeRestoreCode()
                },
                onDone: {
                    showSyncSetup = false
                },
            )
        }
        .alert("Disconnect sync?", isPresented: $showDisconnectConfirm) {
            Button("Disconnect", role: .destructive) {
                disconnect()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(disconnectMessage)
        }
        .alert(
            "Remove this library from this Mac?",
            isPresented: $showForgetConfirm
        ) {
            Button("Remove", role: .destructive) {
                onForgetLibrary()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(
                Self.forgetConfirmationMessage(
                    hasCloudHome: configStore.config.hasCloudHome,
                    hasPendingCloudWork: outboxStore.hasPendingCloudWork
                )
            )
        }
        .onDisappear {
            disconnectWarningTask?.cancel()
        }
    }

    /// Section footer for the remove-library control. A synced library's cloud
    /// copy survives and can be restored; a never-synced library's catalog is
    /// gone for good, though the audio files bae indexed in place are left
    /// alone.
    private var removeFooter: String {
        if configStore.config.hasCloudHome {
            return String(
                localized:
                    "Removes this library and its downloaded files from this Mac. Your library in the cloud is untouched — you can restore it here later."
            )
        }
        return String(
            localized:
                "This library isn't synced. Removing it permanently deletes its catalog — albums, metadata edits, and play history. Audio files in your folders aren't deleted."
        )
    }

    /// Body text for the remove-library confirmation. A synced library's cloud
    /// copy survives and can be restored later; queued cloud writes that
    /// haven't landed are called out because they are lost with the local
    /// data. A never-synced library's catalog is gone for good — though the
    /// audio files bae indexed in place stay in the user's folders. The
    /// pending-work flag is ignored without a cloud home (no outbox exists).
    static func forgetConfirmationMessage(
        hasCloudHome: Bool,
        hasPendingCloudWork: Bool
    ) -> String {
        guard hasCloudHome else {
            return String(
                localized:
                    "This library has never been synced. Its catalog will be permanently deleted; audio files in your folders aren't deleted."
            )
        }
        let base = String(
            localized:
                "Your library in the cloud is untouched — you can restore it from the welcome screen later."
        )
        guard hasPendingCloudWork else { return base }
        let extra = String(
            localized:
                "Some changes haven't finished uploading and will be lost."
        )
        return "\(base) \(extra)"
    }

    /// Body text for the disconnect confirmation. bae-core pre-formats the
    /// data-loss warning when releases live only in the cloud; the view just
    /// stitches it onto the base sentence.
    private var disconnectMessage: String {
        let base = String(
            localized:
                "This will stop syncing and remove the cloud provider configuration."
        )
        guard let extra = disconnectExtraWarning else { return base }
        return "\(base) \(extra)"
    }

    // MARK: - Actions

    /// Query the disconnect warning text, then surface the confirmation
    /// alert. If the count itself fails, render the error inline (so the
    /// user knows they're proceeding without the data-loss check) and
    /// still open the alert so the user can choose to continue or cancel.
    private func promptDisconnect() {
        error = nil
        disconnectWarningTask?.cancel()
        disconnectWarningTask = Task {
            do {
                disconnectExtraWarning =
                    try await sync.disconnectWarningMessage()
            }
            catch is CancellationError {
                return
            }
            catch {
                logger.error(
                    "Failed to compute disconnect warning: \(error.localizedDescription)"
                )
                self.error = String(
                    localized:
                        "Couldn't check for cloud-only releases: \(error.localizedDescription)"
                )
                disconnectExtraWarning = nil
            }
            showDisconnectConfirm = true
        }
    }

    private func disconnect() {
        do {
            try sync.disconnectCloudProvider()
            error = nil
            KeychainService.deleteRestoreCode(
                libraryId: configStore.config.libraryId
            )
        }
        catch {
            logger.error("Failed to disconnect: \(error.localizedDescription)")
            self.error = String(
                localized:
                    "Failed to disconnect: \(error.localizedDescription)"
            )
        }
    }

    private func storeRestoreCode() {
        sync.storeRestoreCodeInKeychain(
            libraryId: configStore.config.libraryId,
            onError: { [uiStore] in uiStore.showError($0) }
        )
    }
}

/// The "Recovery" section: reveals the library's recovery code on demand. The
/// recovery code is a bearer credential — anyone holding it gains full access —
/// so it's kept behind a button and labelled as sensitive, used only to restore
/// on a new device when no existing device is available to approve a join. The
/// sheet's `.task` owns the generation lifecycle: it fires on appear and is
/// cancelled automatically when the sheet dismisses (which propagates through
/// `withTaskCancellationHandler` to the off-main worker).
private struct RecoveryCodeSection: View {
    let generate: @Sendable () throws -> String

    @State
    private var show = false
    @State
    private var result: Result<String, Error>?

    var body: some View {
        Section("Recovery") {
            Text(
                "Your recovery code restores this library on a new device when you have no other device available to approve it. Anyone with it has full access — keep it secret."
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            Button("Show recovery code...") {
                result = nil
                show = true
            }
        }
        .sheet(isPresented: $show) {
            CodeShareSheet(
                result: $result,
                onDismiss: { show = false },
            )
            .task { await runGenerate() }
        }
    }

    private func runGenerate() async {
        let generate = generate
        do {
            let code = try await DetachedWork.run { try generate() }
            result = .success(code)
        }
        catch is CancellationError {
            logger.debug("recovery code generation cancelled")
        }
        catch {
            logger.error(
                "Failed to generate recovery code: \(error.localizedDescription)"
            )
            result = .failure(error)
        }
    }
}
