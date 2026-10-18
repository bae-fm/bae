import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("LibrarySettings")

struct LibrarySettingsTab: View {
    /// Removes the active library from this device.
    let onForgetLibrary: () -> Void
    /// Shows the device-reset warning for the active library.
    let onResetDevice: () -> Void
    /// Asks Sparkle for a newer version of the app.
    let onCheckForUpdates: () -> Void

    @Environment(Sync.self)
    var sync
    @Environment(ConfigStore.self)
    var configStore
    @Environment(OutboxStore.self)
    var outboxStore

    @State
    private var showSyncSetup = false
    @State
    private var showForgetConfirm = false

    var body: some View {
        Form {
            Section {
                LabeledContent("Name", value: configStore.libraryName)
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
                if let location = configStore.storage {
                    ConnectedProviderControls(
                        location: location,
                        sync: sync,
                        configStore: configStore
                    )
                    SyncStatusLine(
                        location: location,
                        onResetDevice: onResetDevice,
                        onCheckForUpdates: onCheckForUpdates
                    )
                    UploadPauseToggle()
                }
                else {
                    Button("Set up sync...") {
                        showSyncSetup = true
                    }
                }
            }

            if configStore.hasCloudHome {
                MembersSection()
                HistorySharingSection()
                RestoreCodeSection(generate: sync.generateRestoreCode)
            }

            Section("Remove") {
                Text(removeFooter)
                    .themeText(.detail)
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
            SyncSetupWizard(onDone: { showSyncSetup = false })
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
                LibraryRemovalConfirmation.message(
                    hasCloudHome: configStore.hasCloudHome,
                    hasPendingCloudWork: outboxStore.hasPendingCloudWork
                )
            )
        }
    }

    /// The remove section's footer, which depends on whether the library syncs.
    private var removeFooter: String {
        if configStore.hasCloudHome {
            return String(
                localized:
                    "Removes this Mac from the library's members, then deletes the library and its downloaded files from this Mac. Your library in the cloud is untouched. Removing needs a connection to your storage."
            )
        }
        return String(
            localized:
                "This library isn't synced. Removing it permanently deletes its catalog: albums, metadata edits, and play history. Audio files in your folders aren't deleted."
        )
    }
}

/// The connected provider's details and disconnect flow.
private struct ConnectedProviderControls: View {
    let location: BridgeStorageLocation

    @State
    private var flow: DisconnectSyncFlow

    init(
        location: BridgeStorageLocation,
        sync: Sync,
        configStore: ConfigStore
    ) {
        self.location = location
        _flow = State(
            initialValue: DisconnectSyncFlow(
                cloudOnlyReleaseCount: sync.cloudOnlyReleaseCount,
                atRiskMessage: { count in
                    String.localizedStringWithFormat(
                        NSLocalizedString(
                            "core.sync.cloud_only_releases",
                            tableName: "Core",
                            bundle: .main,
                            comment: ""
                        ),
                        count
                    )
                },
                disconnect: { [sync, configStore] in
                    try await sync.disconnectCloudProvider()
                    configStore.applyStorage(try await sync.storageLocation())
                },
                baseMessage: {
                    String(
                        localized:
                            "This Mac stops syncing and removes its storage credentials. Sign in again to resume syncing."
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
                location: location,
                onDisconnect: { flow.promptDisconnect() }
            )

            if let error = flow.error {
                ErrorText(error)
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

/// Pausing keeps every queued upload in its place; metadata keeps syncing.
private struct UploadPauseToggle: View {
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        Toggle(
            "Pause uploads",
            isOn: Binding(
                get: { outboxStore.snapshot.pauseRequested },
                set: { paused in
                    Task {
                        do { try await sync.setSyncPaused(paused) }
                        catch {
                            logger.error(
                                "Failed to set upload pause: \(error.localizedDescription)"
                            )
                            uiStore.showError(error)
                        }
                    }
                }
            )
        )
    }
}

/// The "Restore code" section: the code that adds this person's other
/// devices, shown only on request.
private struct RestoreCodeSection: View {
    let generate: @Sendable () async throws -> BridgeRestoreCodeShare

    @State
    private var show = false
    @State
    private var result: Result<BridgeRestoreCodeShare, Error>?

    var body: some View {
        Section("Restore code") {
            Text(
                "Your restore code adds your other devices to this library. Anyone with it and access to your storage can open the library. Keep it secret."
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
            Button("Show restore code...") {
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
            let share = try await generate()
            result = .success(share)
        }
        catch is CancellationError {
            logger.debug("restore code generation cancelled")
        }
        catch {
            logger.error(
                "Failed to generate restore code: \(error.localizedDescription)"
            )
            // No line means core reported a cancellation; keep the spinner.
            guard DisplayError(error) != nil else { return }
            result = .failure(error)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    @MainActor
    private func librarySettingsTabPreview(
        configStore: ConfigStore
    ) -> some View {
        LibrarySettingsTab(
            onForgetLibrary: {},
            onResetDevice: {},
            onCheckForUpdates: {}
        )
        .frame(width: 500, height: 640)
        .environment(PreviewData.previewSync())
        #if BAE_OAUTH_PROVIDERS
            .environment(CloudSyncSetup())
        #endif
        .environment(configStore)
        .environment(
            SyncStatusStore(
                snapshot: BridgeSyncStatusSnapshot(
                    error: nil,
                    connected: true,
                    running: false,
                    pending: .ready(records: []),
                    indicator: .synced
                )
            )
        )
        .environment(
            MembershipStore.stub(
                people: PreviewData.members,
                selfIsAdmin: true
            )
        )
        .environment(UiStore())
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
    }

    #Preview("Not connected") {
        librarySettingsTabPreview(configStore: PreviewData.configStore())
    }

    #Preview("Connected") {
        librarySettingsTabPreview(
            configStore: PreviewData.connectedConfigStore()
        )
    }
#endif
