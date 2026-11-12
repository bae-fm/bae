import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("LibrarySettings")

struct LibrarySettingsTab: View {
    /// Shows the screen that offers resetting this device for the active
    /// library.
    let onResetDevice: () -> Void
    /// Closes the active library and asks for the person's restore code to
    /// restore it over this device's copy.
    let onRestoreFromCode: () -> Void
    /// Asks Sparkle for a newer version of the app.
    let onCheckForUpdates: () -> Void
    /// Opens Rename Library on the active library's name.
    let onRenameLibrary: () -> Void
    /// Closes the active library for the welcome list, where it is removed
    /// from this Mac.
    let onRemoveLibrary: () -> Void

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
    /// The pages open over the Library pane.
    @State
    private var path: [Page] = []
    /// The restore code section's reveal, closed whenever this pane goes away.
    @State
    private var restoreCode = RestoreCodeReveal()
    /// Why the library's location couldn't be read as this pane opened.
    @State
    private var storageReadError: DisplayError?

    /// The pages the Library pane opens.
    private enum Page: Hashable {
        case sync, people
    }

    var body: some View {
        NavigationStack(path: $path) {
            Form {
                Section {
                    LabeledContent("Name") {
                        HStack {
                            Text(libraryName.name ?? "")
                                .foregroundStyle(.secondary)
                            if canRename {
                                Button("Rename…", action: onRenameLibrary)
                            }
                        }
                    }
                    LabeledContent("Library folder") {
                        Button("Show in Finder") {
                            SystemActions.revealInFinder(
                                path: configStore.config.libraryPath
                            )
                        }
                    }
                }

                AdminChangesSection(place: .library)

                Section {
                    syncRow
                    if configStore.syncs {
                        NavigationLink(value: Page.people) {
                            LabeledContent("People and devices") {
                                if let count = peopleAndDevices {
                                    Text(count)
                                }
                            }
                        }
                    }
                } footer: {
                    if !configStore.syncs, !setupRunning {
                        Text("This library is only on this Mac.")
                    }
                }
            }
            .formStyle(.grouped)
            .navigationDestination(for: Page.self) { page in
                switch page {
                case .sync:
                    SyncSettingsPage(
                        restoreCode: restoreCode,
                        storageReadError: storageReadError,
                        onResetDevice: onResetDevice,
                        onRestoreFromCode: onRestoreFromCode,
                        onCheckForUpdates: onCheckForUpdates,
                        onRemoveLibrary: onRemoveLibrary
                    )
                case .people:
                    PeopleAndDevicesDestination()
                }
            }
        }
        // The name and the people are read while this pane shows, which
        // includes the pages it opens: the stack, not a page, owns that.
        .task { await libraryName.follow() }
        .task { await readStorage() }
        .onAppear { membership.show() }
        // The restore code is made while this pane shows a library that
        // syncs, and dropped when it stops.
        .onChange(of: configStore.syncs, initial: true) { _, has in
            if has {
                restoreCode.open(sync.generateRestoreCode)
            }
            else {
                restoreCode.close()
            }
        }
        .onDisappear {
            restoreCode.close()
            membership.hide()
        }
        .sheet(isPresented: $showSyncSetup) {
            // Setup ends on the Sync page, which holds the restore code.
            SyncSetupWizard(onDone: {
                showSyncSetup = false
                path = [.sync]
            })
        }
    }

    /// Whether a storage setup runs that bae finishes on its own, such as one
    /// it was stopped partway through.
    private var setupRunning: Bool {
        syncStatus.snapshot?.setupRunning == true
    }

    /// An admin renames a synced library; a library only on this Mac is its
    /// person's to rename.
    private var canRename: Bool {
        !configStore.syncs || membership.selfIsAdmin
    }

    /// The Sync row: the state of the sync, opening its page; without
    /// storage, Set up sync….
    @ViewBuilder
    private var syncRow: some View {
        if configStore.syncs || setupRunning {
            NavigationLink(value: Page.sync) {
                LabeledContent("Sync") {
                    if let shown = syncStatus.shown {
                        ToneDotValue(
                            text: shown.statusLine,
                            tone: shown.statusTone
                        )
                    }
                }
            }
        }
        else {
            NavigationLink(value: Page.sync) {
                LabeledContent("Sync") {
                    HStack {
                        Text("Not synced")
                            .foregroundStyle(.secondary)
                        Button("Set up sync…") {
                            showSyncSetup = true
                        }
                    }
                }
            }
        }
    }

    /// "2 people · 3 devices", once the people have been read.
    private var peopleAndDevices: String? {
        guard let members = membership.members else { return nil }
        let devices = members.reduce(0) {
            $0 + $1.membership.activeDevices.count
        }
        let people = String(localized: "\(members.count) people")
        let deviceCount = String(localized: "\(devices) devices")
        return "\(people) · \(deviceCount)"
    }

    /// Read the library's location again as the pane opens: coven records
    /// a Dropbox or OneDrive folder's name at every connection, so one
    /// renamed at the provider shows from the first read after.
    private func readStorage() async {
        storageReadError = nil
        do {
            configStore.applyStorage(try await sync.storageLocation())
        }
        catch {
            // The location is this device's own record, so a read that fails
            // is a bug, not a condition the person can fix. A cancellation as
            // the pane closes has no line to show and is not one.
            guard let display = DisplayError(error) else { return }
            let detail = String(reflecting: error)
            logger.error("Couldn't read the library's location: \(detail)")
            CoreBugReporter()
                .report(
                    operation: "read the library's cloud home",
                    detail: detail
                )
            storageReadError = display
        }
    }
}

/// The Sync page: the state sync is in, with its cause, its fix and when it
/// last finished, then where the library is stored, its restore code and what
/// sync waits on.
private struct SyncSettingsPage: View {
    let restoreCode: RestoreCodeReveal
    let storageReadError: DisplayError?
    let onResetDevice: () -> Void
    let onRestoreFromCode: () -> Void
    let onCheckForUpdates: () -> Void
    let onRemoveLibrary: () -> Void

    @Environment(ConfigStore.self)
    private var configStore

    var body: some View {
        Form {
            Section {
                UndoneSetupLine()
                if let storageReadError {
                    Notice(
                        title: String(
                            localized:
                                "Couldn’t read this library’s cloud home."
                        ),
                        advice: nil,
                        error: storageReadError
                    )
                }
                SyncStatusBlock(
                    location: configStore.storage,
                    actions: SyncFixActions(
                        onResetDevice: onResetDevice,
                        onCheckForUpdates: onCheckForUpdates,
                        onRemoveLibrary: onRemoveLibrary
                    )
                )
            }
            if let location = configStore.storage {
                Section("Cloud home") {
                    CloudProviderConnectedSection(location: location)
                    if let folder = location.folderLabel {
                        OpenFolderInBrowserRow(folder: folder)
                    }
                }
                LibraryRestoreCodeSection(reveal: restoreCode)
                SyncPendingList(
                    location: location,
                    onRestoreFromCode: onRestoreFromCode,
                    onCheckForUpdates: onCheckForUpdates
                )
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Sync")
    }
}

/// The People and devices page, which has its own header and back button
/// for the pages inside it, so the stack's is hidden.
private struct PeopleAndDevicesDestination: View {
    @Environment(\.dismiss)
    private var dismiss

    var body: some View {
        PeopleAndDevicesPage(onBack: { dismiss() })
            .navigationBarBackButtonHidden(true)
    }
}

/// The restore code section as Library settings shows it, on the Sync page:
/// printing names where the library is stored.
private struct LibraryRestoreCodeSection: View {
    let reveal: RestoreCodeReveal

    @Environment(ConfigStore.self)
    private var configStore

    var body: some View {
        RestoreCodeSection(
            reveal: reveal,
            onPrint: { code in
                RestoreCodePrinting.print(
                    code: code,
                    location: configStore.storage?.accountLine ?? ""
                )
            }
        )
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
    }

    /// The Library tab over `fixture`'s library, its sync as `shown` says.
    @MainActor
    func librarySettingsTabPreview(
        _ fixture: LibrarySettingsTabPreviewFixture,
        shown: BridgeSyncShown = .previewSynced
    ) -> some View {
        LibrarySettingsTab(
            onResetDevice: {},
            onRestoreFromCode: {},
            onCheckForUpdates: {},
            onRenameLibrary: {},
            onRemoveLibrary: {}
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
                    shown: shown,
                    pending: [],
                    setupRunning: false,
                    undoneSetup: nil
                )
            )
        )
        .environment(fixture.membership)
        .environment(UiStore())
        .environment(fixture.libraryName)
    }

    /// The Sync page over `fixture`'s library, its sync as `shown` says, with
    /// what coven waits on, a storage setup that ended undone, and a failed
    /// read of the library's location, when given, and its restore code
    /// masked unless `restoreCode` says otherwise.
    @MainActor
    func syncSettingsPagePreview(
        _ fixture: LibrarySettingsTabPreviewFixture,
        shown: BridgeSyncShown,
        pending: [BridgePendingRecord] = [],
        undoneSetup: BridgeUndoneStorageSetup? = nil,
        storageReadError: DisplayError? = nil,
        restoreCode: RestoreCodeReveal.State = .masked(
            code: PreviewData.restoreCodeGrouped
        )
    ) -> some View {
        NavigationStack {
            SyncSettingsPage(
                restoreCode: RestoreCodeReveal(previewState: restoreCode),
                storageReadError: storageReadError,
                onResetDevice: {},
                onRestoreFromCode: {},
                onCheckForUpdates: {},
                onRemoveLibrary: {}
            )
        }
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
                    shown: shown,
                    pending: pending,
                    setupRunning: false,
                    undoneSetup: undoneSetup
                )
            )
        )
        .environment(UiStore())
    }

    #Preview("No storage") {
        librarySettingsTabPreview(
            LibrarySettingsTabPreviewFixture(storage: nil),
            shown: .previewNotSynced
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
            shown: .previewSignedOut
        )
    }

    #Preview("Sync page") {
        syncSettingsPagePreview(
            LibrarySettingsTabPreviewFixture(
                storage: PreviewData.previewStorage
            ),
            shown: .previewSynced
        )
    }

    /// The restore code section with its reveal at `state`, in an S3 library.
    @MainActor
    private func restoreCodeSectionPreview(
        _ state: RestoreCodeReveal.State
    ) -> some View {
        Form {
            RestoreCodeSection(
                reveal: RestoreCodeReveal(previewState: state),
                onPrint: { _ in }
            )
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

    #Preview("Restore code loading") {
        restoreCodeSectionPreview(.loading)
    }

    #Preview("Restore code masked") {
        restoreCodeSectionPreview(
            .masked(code: PreviewData.restoreCodeGrouped)
        )
    }

    #Preview("Restore code shown") {
        restoreCodeSectionPreview(.shown(code: PreviewData.restoreCodeGrouped))
    }

    #Preview("Restore code failed") {
        restoreCodeSectionPreview(
            .failed(DisplayError(line: "Couldn't reach the cloud."))
        )
    }
#endif
