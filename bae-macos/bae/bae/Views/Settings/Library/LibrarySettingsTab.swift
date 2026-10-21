import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("LibrarySettings")

struct LibrarySettingsTab: View {
    /// What removing the active library would lose, read before asking.
    let unsentBeforeRemoval: () async throws -> BridgeUnsentWork
    /// Removes the active library from this device.
    let onForgetLibrary: () -> Void
    /// Shows the device-reset warning for the active library.
    let onResetDevice: () -> Void
    /// Closes the active library and asks for the person's restore code to
    /// restore it over this device's copy.
    let onRestoreFromCode: () -> Void
    /// Asks Sparkle for a newer version of the app.
    let onCheckForUpdates: () -> Void

    @Environment(Sync.self)
    var sync
    @Environment(ConfigStore.self)
    var configStore

    @State
    private var showSyncSetup = false
    /// What removal would lose while the person is asked to confirm it.
    @State
    private var removalUnsent: BridgeUnsentWork?
    /// Why what removal would lose couldn't be read.
    @State
    private var removalError: DisplayError?
    /// The recovery code section's reveal, hidden whenever this tab goes away.
    @State
    private var recoveryCode = RecoveryCodeReveal()

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
                        onRestoreFromCode: onRestoreFromCode,
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
                RecoveryCodeSection(reveal: recoveryCode)
            }

            Section("Remove") {
                Text(removeFooter)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                Button(
                    "Remove this library from this Mac...",
                    role: .destructive
                ) {
                    askToRemove()
                }
                if let removalError {
                    ErrorDetailDisclosure(error: removalError)
                }
            }
        }
        .formStyle(.grouped)
        .onDisappear { recoveryCode.hide() }
        .sheet(isPresented: $showSyncSetup) {
            SyncSetupWizard(onDone: { showSyncSetup = false })
        }
        .alert(
            "Remove this library from this Mac?",
            isPresented: Binding(
                get: { removalUnsent != nil },
                set: { if !$0 { removalUnsent = nil } }
            ),
            presenting: removalUnsent
        ) { _ in
            Button("Remove", role: .destructive) {
                onForgetLibrary()
            }
            Button("Cancel", role: .cancel) {}
        } message: { unsent in
            Text(
                unsent.confirmation(
                    consequence: LibraryRemovalConfirmation.consequence,
                    message: LibraryRemovalConfirmation.message(
                        hasCloudHome: configStore.hasCloudHome
                    )
                )
            )
        }
    }

    /// Read what removal would lose, then ask. The library keeps running.
    private func askToRemove() {
        removalError = nil
        Task {
            do {
                removalUnsent = try await unsentBeforeRemoval()
            }
            catch {
                logger.error(
                    "Couldn't read what removing the library loses: \(error.localizedDescription)"
                )
                removalError = DisplayError(error)
            }
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
                ErrorDetailDisclosure(error: error)
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

#if DEBUG
    // MARK: - Previews

    @MainActor
    private func librarySettingsTabPreview(
        configStore: ConfigStore
    ) -> some View {
        LibrarySettingsTab(
            unsentBeforeRemoval: { BridgeUnsentWork(writes: 0, files: 0) },
            onForgetLibrary: {},
            onResetDevice: {},
            onRestoreFromCode: {},
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

    /// The recovery code section with its reveal at `state`, in an S3 library.
    @MainActor
    private func recoveryCodeSectionPreview(
        _ state: RecoveryCodeReveal.State
    ) -> some View {
        Form {
            RecoveryCodeSection(reveal: RecoveryCodeReveal(previewState: state))
        }
        .formStyle(.grouped)
        .frame(width: 500, height: 300)
        .environment(PreviewData.previewSync())
        .environment(PreviewData.connectedConfigStore())
    }

    #Preview("Recovery code loading") {
        recoveryCodeSectionPreview(.loading)
    }

    #Preview("Recovery code shown") {
        recoveryCodeSectionPreview(.shown(code: PreviewData.recoveryCode))
    }

    #Preview("Recovery code failed") {
        recoveryCodeSectionPreview(
            .failed(DisplayError(line: "Couldn't reach your storage."))
        )
    }
#endif
