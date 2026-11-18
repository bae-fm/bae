import BaeKit
import SwiftUI

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
            // The pane's title over the first page; the pages inside it have
            // their own.
            .safeAreaInset(edge: .top, spacing: 0) {
                SettingsPaneHeading { Text(SettingsTab.library.title) }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .windowBackground()
            }
            .navigationDestination(for: Page.self) { page in
                switch page {
                case .sync:
                    SyncSettingsPage(
                        restoreCode: restoreCode,
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
        .onAppear {
            membership.show()
            membership.showAdminChanges(at: .library)
        }
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
            membership.hideAdminChanges(at: .library)
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
        guard let people = membership.people else { return nil }
        let peopleText = String(localized: "\(Int(people.people)) people")
        let deviceCount = String(localized: "\(Int(people.devices)) devices")
        return "\(peopleText) · \(deviceCount)"
    }
}

/// The Sync page: the state sync is in, with its cause, its fix and when it
/// last finished, then where the library is stored, its restore code and what
/// sync waits on.
private struct SyncSettingsPage: View {
    let restoreCode: RestoreCodeReveal
    let onResetDevice: () -> Void
    let onRestoreFromCode: () -> Void
    let onCheckForUpdates: () -> Void
    let onRemoveLibrary: () -> Void

    @Environment(ConfigStore.self)
    private var configStore
    @Environment(\.dismiss)
    private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            SettingsPageHeader(
                title: String(localized: "Sync"),
                backTitle: String(localized: "Library"),
                onBack: { dismiss() }
            )
            form
        }
        .navigationBarBackButtonHidden(true)
    }

    private var form: some View {
        Form {
            Section {
                UndoneSetupLine()
                SyncStatusBlock(
                    location: configStore.storage,
                    actions: SyncFixActions(
                        onResetDevice: onResetDevice,
                        onRestoreFromCode: onRestoreFromCode,
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
                LibraryRestoreCodeSection(
                    reveal: restoreCode,
                    location: location
                )
                SyncPendingList(
                    location: location,
                    onRestoreFromCode: onRestoreFromCode,
                    onCheckForUpdates: onCheckForUpdates
                )
            }
        }
        .formStyle(.grouped)
    }
}

/// The People and devices page, which has its own header and back button
/// for the pages inside it, so the stack's is hidden.

#if DEBUG
    // periphery:ignore - called by baeShots
    /// The Sync page with actions that do nothing, for a shot.
    @MainActor
    func syncSettingsPageWithInertActions(
        restoreCode: RestoreCodeReveal
    ) -> some View {
        SyncSettingsPage(
            restoreCode: restoreCode,
            onResetDevice: {},
            onRestoreFromCode: {},
            onCheckForUpdates: {},
            onRemoveLibrary: {}
        )
    }
#endif

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
    let location: BridgeStorageLocation

    var body: some View {
        RestoreCodeSection(
            reveal: reveal,
            onPrint: { code in
                RestoreCodePrinting.print(
                    code: code,
                    location: location.accountLine
                )
            }
        )
    }
}
