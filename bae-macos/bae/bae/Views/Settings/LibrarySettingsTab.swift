import SwiftUI
import os.log

private let logger = Logger.bae("LibrarySettings")

struct LibrarySettingsTab: View {
    @Environment(Sync.self)
    var sync
    @Environment(ConfigStore.self)
    var configStore
    @Environment(UiStore.self)
    var uiStore

    @State
    private var error: String?
    @State
    private var showSyncSetup = false
    @State
    private var showDisconnectConfirm = false
    /// Captured at the moment the user clicks Disconnect: bae-core's
    /// pre-formatted warning when releases live only in the cloud (`nil`
    /// when no releases are at risk).
    @State
    private var disconnectExtraWarning: String? = nil

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
                ConnectDeviceSection(generate: sync.generateRestoreCode)
            }
        }
        .formStyle(.grouped)
        .sheet(isPresented: $showSyncSetup) {
            SyncSetupWizard(
                onConnectS3: { config in
                    try await sync.saveSyncConfig(config)
                    storeRestoreCode()
                },
                onConnectOAuth: { provider in
                    try await sync.signInCloudProvider(provider)
                    storeRestoreCode()
                },
                onConnectCloudKit: {
                    try await sync.connectCloudkit()
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
    }

    /// Body text for the disconnect confirmation. bae-core pre-formats the
    /// data-loss warning when releases live only in the cloud; the view just
    /// stitches it onto the base sentence.
    private var disconnectMessage: String {
        let base =
            "This will stop syncing and remove the cloud provider configuration."
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
        do {
            disconnectExtraWarning = try sync.disconnectWarningMessage()
        }
        catch {
            logger.error(
                "Failed to compute disconnect warning: \(error.localizedDescription)"
            )
            self.error =
                "Couldn't check for cloud-only releases: \(error.localizedDescription)"
            disconnectExtraWarning = nil
        }
        showDisconnectConfirm = true
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
            self.error = "Failed to disconnect: \(error.localizedDescription)"
        }
    }

    private func storeRestoreCode() {
        sync.storeRestoreCodeInKeychain(
            libraryId: configStore.config.libraryId,
            onError: { [uiStore] in uiStore.showError($0) }
        )
    }
}

/// The "Devices" section: a "Connect another device..." button that opens a
/// sheet showing a pairing code to scan or paste on another device. The sheet's
/// `.task` owns the generation lifecycle: it fires on appear and is
/// cancelled automatically when the sheet dismisses (which propagates
/// through `withTaskCancellationHandler` to the off-main worker).
private struct ConnectDeviceSection: View {
    let generate: @Sendable () throws -> String

    @State
    private var show = false
    @State
    private var result: Result<String, Error>? = nil

    var body: some View {
        Section("Devices") {
            Button("Connect another device...") {
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
            let detached = Task.detached { try generate() }
            let code = try await withTaskCancellationHandler {
                try await detached.value
            } onCancel: {
                detached.cancel()
            }
            result = .success(code)
        }
        catch is CancellationError {
            logger.debug("pairing code generation cancelled")
        }
        catch {
            logger.error(
                "Failed to generate pairing code: \(error.localizedDescription)"
            )
            result = .failure(error)
        }
    }
}
