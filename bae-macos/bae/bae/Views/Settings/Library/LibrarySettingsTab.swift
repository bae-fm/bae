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

    @Environment(ConfigStore.self)
    var configStore
    @Environment(SyncStatusStore.self)
    private var syncStatus
    @Environment(LibraryNameStore.self)
    private var libraryName
    @Environment(MembershipStore.self)
    private var membership

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
                LabeledContent("Name", value: libraryName.name ?? "")
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
                UndoneSetupLine()
                if let location = configStore.storage {
                    CloudProviderConnectedSection(location: location)
                    if syncStatus.line == .noStorage {
                        StorageSignIn(location: location)
                        SyncFailureLines(onResetDevice: onResetDevice)
                    }
                    else {
                        SyncStatusLine(
                            location: location,
                            onResetDevice: onResetDevice,
                            onRestoreFromCode: onRestoreFromCode,
                            onCheckForUpdates: onCheckForUpdates
                        )
                        UploadPauseToggle()
                    }
                }
                else if syncStatus.snapshot?.setupRunning == true {
                    // A setup bae finishes on its own, such as one it was
                    // stopped partway through, records the location as it
                    // ends.
                    if let statusText = syncStatus.statusText {
                        Text(statusText)
                    }
                    SyncFailureLines(onResetDevice: onResetDevice)
                }
                else {
                    Button("Set up sync…") {
                        showSyncSetup = true
                    }
                }
            }

            if configStore.hasCloudHome {
                MembersSection()
                AdminChangesSection()
                HistorySharingSection()
                RecoveryCodeSection(reveal: recoveryCode)
            }

            Section("Remove") {
                Text(removeFooter)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                Button(
                    "Remove this library from this Mac…",
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
        // The name, the people, invites and admin changes are read while
        // this tab shows.
        .task { await libraryName.follow() }
        .onAppear { membership.show() }
        .onDisappear {
            recoveryCode.hide()
            membership.hide()
        }
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
                    "Couldn't read what removing the library loses: \(String(reflecting: error))"
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
                    "Removes this Mac from the library's members, then deletes the library and its downloaded files from this Mac. The library will remain in storage and on your other devices. Removing needs a connection to your storage."
            )
        }
        return String(
            localized:
                "This library isn't synced. Removing it permanently deletes its catalog: albums, metadata edits, and play history. Audio files in your folders aren't deleted."
        )
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
                get: { outboxStore.uploadsPaused },
                set: { paused in
                    Task {
                        do { try await sync.setSyncPaused(paused) }
                        catch {
                            logger.error(
                                "Failed to set upload pause: \(String(reflecting: error))"
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

    /// The Library tab over `configStore`'s library, its sync at `indicator`.
    @MainActor
    func librarySettingsTabPreview(
        configStore: ConfigStore,
        indicator: BridgeSyncIndicator? = .synced
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
                    pending: [],
                    indicator: indicator,
                    reconnectOffered: false,
                    setupRunning: false,
                    undoneSetup: nil
                )
            )
        )
        .environment(
            MembershipStore.stub(
                people: PreviewData.members,
                selfIsAdmin: true
            )
        )
        .environment(SyncActivityStore.preview(needsAttention: false))
        .environment(UiStore())
        .environment(OutboxStore(uploadsPaused: false))
        .environment(LibraryNameStore.stub(name: "Preview Library"))
    }

    #Preview("No cloud home") {
        librarySettingsTabPreview(
            configStore: PreviewData.configStore(),
            indicator: nil
        )
    }

    #Preview("Synced") {
        librarySettingsTabPreview(
            configStore: PreviewData.connectedConfigStore(
                storage: PreviewData.previewStorage
            )
        )
    }

    #Preview("Dropbox") {
        librarySettingsTabPreview(
            configStore: PreviewData.connectedConfigStore(
                storage: PreviewData.previewDropboxStorage
            )
        )
    }

    #Preview("OneDrive") {
        librarySettingsTabPreview(
            configStore: PreviewData.connectedConfigStore(
                storage: PreviewData.previewOneDriveStorage
            )
        )
    }

    #Preview("Credentials gone") {
        librarySettingsTabPreview(
            configStore: PreviewData.connectedConfigStore(
                storage: PreviewData.previewStorage
            ),
            indicator: nil
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
        .environment(
            PreviewData.connectedConfigStore(
                storage: PreviewData.previewStorage
            )
        )
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
