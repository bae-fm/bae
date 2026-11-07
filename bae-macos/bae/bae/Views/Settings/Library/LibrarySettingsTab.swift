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
    @Environment(Sync.self)
    private var sync

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
    /// Why the library's location couldn't be read as this tab opened.
    @State
    private var storageReadError: DisplayError?

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
                if let storageReadError {
                    ErrorDetailDisclosure(error: storageReadError)
                }
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

            if configStore.syncs {
                MembersSection()
                AdminChangesSection()
                HistorySharingSection()
                RecoveryCodeSection(reveal: recoveryCode)
            }

            Section("Remove") {
                Text(removeFooter)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                Button(removeButtonTitle, role: .destructive) {
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
        .task { await readStorage() }
        .onAppear { membership.show() }
        .onDisappear {
            recoveryCode.hide()
            membership.hide()
        }
        .sheet(isPresented: $showSyncSetup) {
            SyncSetupWizard(onDone: { showSyncSetup = false })
        }
        .alert(
            ending.title,
            isPresented: Binding(
                get: { removalUnsent != nil },
                set: { if !$0 { removalUnsent = nil } }
            ),
            presenting: removalUnsent
        ) { _ in
            Button(ending.action, role: .destructive) {
                onForgetLibrary()
            }
            Button("Cancel", role: .cancel) {}
        } message: { unsent in
            Text(
                unsent.confirmation(
                    consequence: LibraryRemovalConfirmation.consequence,
                    message: LibraryRemovalConfirmation.message(
                        syncs: configStore.syncs
                    )
                )
            )
        }
    }

    /// Read the library's location again as the tab opens: coven records
    /// a Dropbox or OneDrive folder's name at every connection, so one
    /// renamed at the provider shows from the first read after.
    private func readStorage() async {
        storageReadError = nil
        do {
            configStore.applyStorage(try await sync.storageLocation())
        }
        catch {
            logger.error(
                "Couldn't read the library's location: \(String(reflecting: error))"
            )
            storageReadError = DisplayError(error)
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

    /// Whether the active library is removed or, never synced, deleted.
    private var ending: LibraryRemovalConfirmation.Ending {
        LibraryRemovalConfirmation.Ending(
            syncs: configStore.syncs
        )
    }

    private var removeButtonTitle: String {
        switch ending {
        case .remove: String(localized: "Remove this library from this Mac…")
        case .delete: String(localized: "Delete this library…")
        }
    }

    /// The remove section's footer, which depends on whether the library syncs.
    private var removeFooter: String {
        if configStore.syncs {
            return String(
                localized:
                    "Removes this Mac from the library's members, then deletes the library and its downloaded files from this Mac. The library will remain in storage and on your other devices. Removing needs a connection to your storage."
            )
        }
        return LibraryRemovalConfirmation.message(syncs: false)
    }
}

#if DEBUG
    // MARK: - Previews

    /// The Library tab's preview stores over a library whose storage is
    /// `storage`, or none: its configuration, and what the tab reads off the
    /// main thread, the library's name and its people, each answered a step
    /// after the tab first draws. The people reach the library as `storage`'s
    /// members do; a library with no storage has none.
    @MainActor
    struct LibrarySettingsTabPreviewFixture {
        let configStore: ConfigStore
        let libraryName = LibraryNameStore.stub(name: "Preview Library")
        let membership: MembershipStore

        init(storage: BridgeStorageLocation?) {
            configStore =
                storage.map { PreviewData.connectedConfigStore(storage: $0) }
                ?? PreviewData.configStore()
            membership = MembershipStore.stub(
                people: storage.map { PreviewData.members(reaching: $0) } ?? [],
                selfIsAdmin: true
            )
        }

        /// Whether both reads have answered, so the tab shows the name and
        /// the people.
        var answered: Bool {
            libraryName.name != nil && membership.members != nil
        }
    }

    /// The Library tab over `fixture`'s library, its sync at `indicator`.
    @MainActor
    func librarySettingsTabPreview(
        _ fixture: LibrarySettingsTabPreviewFixture,
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
        .environment(
            PreviewData.previewSync(storage: fixture.configStore.storage)
        )
        #if BAE_OAUTH_PROVIDERS
            .environment(CloudSyncSetup())
        #endif
        .environment(fixture.configStore)
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
        .environment(fixture.membership)
        .environment(SyncActivityStore.preview(needsAttention: false))
        .environment(UiStore())
        .environment(fixture.libraryName)
    }

    #Preview("No storage") {
        librarySettingsTabPreview(
            LibrarySettingsTabPreviewFixture(storage: nil),
            indicator: nil
        )
    }

    #Preview("Synced") {
        librarySettingsTabPreview(
            LibrarySettingsTabPreviewFixture(
                storage: PreviewData.previewStorage
            )
        )
    }

    #Preview("Dropbox") {
        librarySettingsTabPreview(
            LibrarySettingsTabPreviewFixture(
                storage: PreviewData.previewDropboxStorage
            )
        )
    }

    #Preview("OneDrive") {
        librarySettingsTabPreview(
            LibrarySettingsTabPreviewFixture(
                storage: PreviewData.previewOneDriveStorage
            )
        )
    }

    #Preview("Credentials gone") {
        librarySettingsTabPreview(
            LibrarySettingsTabPreviewFixture(
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
        .environment(
            PreviewData.previewSync(storage: PreviewData.previewStorage)
        )
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
