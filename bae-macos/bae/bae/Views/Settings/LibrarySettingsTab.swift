import BaeKit
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
    private var showSyncSetup = false
    @State
    private var showForgetConfirm = false

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

                if let syncConfig = configStore.config.sync {
                    ConnectedProviderControls(
                        config: syncConfig,
                        sync: sync,
                        libraryId: configStore.config.libraryId
                    )
                }
                else {
                    Button("Set up sync...") {
                        showSyncSetup = true
                    }
                }
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

    private func storeRestoreCode() {
        sync.storeRestoreCodeInKeychain(
            libraryId: configStore.config.libraryId,
            onError: { [uiStore] in uiStore.showError($0) }
        )
    }
}

/// The connected-provider controls in the Sync section: the provider details
/// and the disconnect flow. Split into its own view so it can seed the shared
/// `DisconnectSyncFlow` as `@State` from the sync service and library id —
/// values a parent can't read at `@State` init time because they come from the
/// environment. macOS's base confirmation sentence omits iOS's "pair from
/// another device" note because it has a reconnect flow.
private struct ConnectedProviderControls: View {
    let config: BridgeSyncConfig

    @State
    private var flow: DisconnectSyncFlow

    init(config: BridgeSyncConfig, sync: Sync, libraryId: String) {
        self.config = config
        _flow = State(
            initialValue: DisconnectSyncFlow(
                warningMessage: sync.disconnectWarningMessage,
                disconnect: sync.disconnectCloudProvider,
                deleteRestoreCode: {
                    KeychainService.deleteRestoreCode(libraryId: libraryId)
                },
                baseMessage: {
                    String(
                        localized:
                            "This will stop syncing and remove the cloud provider configuration."
                    )
                },
                warningCheckFailedMessage: {
                    String(
                        localized:
                            "Couldn't check for cloud-only releases: \($0)"
                    )
                },
                disconnectFailedMessage: {
                    String(localized: "Failed to disconnect: \($0)")
                }
            )
        )
    }

    var body: some View {
        @Bindable
        var flow = flow
        Group {
            CloudProviderConnectedSection(
                config: config,
                onDisconnect: { flow.promptDisconnect() }
            )

            if let error = flow.error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
            }
        }
        .alert("Disconnect sync?", isPresented: $flow.showConfirm) {
            Button("Disconnect", role: .destructive) {
                Task { await flow.confirm() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(flow.message)
        }
        .onDisappear { flow.cancelWarningTask() }
    }
}

/// The "Recovery" section: reveals the library's recovery code on demand. The
/// recovery code is a bearer credential — anyone holding it gains full access —
/// so it's kept behind a button and labelled as sensitive, used only to restore
/// on a new device when no existing device is available to approve a join. The
/// sheet's `.task` owns the generation lifecycle: it fires on appear and is
/// cancelled automatically when the sheet dismisses, which propagates through
/// to the underlying Rust future since `generate` is a genuine uniffi async
/// call.
private struct RecoveryCodeSection: View {
    let generate: @Sendable () async throws -> String

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
        do {
            // generateRestoreCode is a genuine uniffi async call: it suspends
            // without blocking a thread, and cancelling this task propagates
            // through to the underlying Rust future.
            let code = try await generate()
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
