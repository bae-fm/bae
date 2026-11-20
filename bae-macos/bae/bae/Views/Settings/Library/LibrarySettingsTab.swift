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
                MacSection {
                    MacRow("Name") {
                        HStack {
                            Text(libraryName.name ?? "")
                                .foregroundStyle(.secondary)
                            if canRename {
                                MacButton("Rename…", action: onRenameLibrary)
                            }
                        }
                    }
                    MacRow("Library folder") {
                        MacButton("Show in Finder") {
                            SystemActions.revealInFinder(
                                path: configStore.config.libraryPath
                            )
                        }
                    }
                }

                AdminChangesSection(place: .library)

                MacSection {
                    syncRow
                    if configStore.syncs {
                        MacRow("People and devices") {
                            if let count = peopleAndDevices {
                                Text(count)
                            }
                        }
                        .pressing(.opens { path.append(.people) })
                    }
                } footer: {
                    if !configStore.syncs, !setupRunning {
                        Text("This library is only on this Mac.")
                    }
                }
            }
            .formStyle(.settings)
            // The pane's title over the first page; the pages inside it have
            // their own.
            .safeAreaInset(edge: .top, spacing: 0) {
                MacPageHeading(title: Text(SettingsTab.library.title))
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
    /// storage, Set up a cloud home….
    @ViewBuilder
    private var syncRow: some View {
        if configStore.syncs || setupRunning {
            MacRow("Sync") {
                if let shown = syncStatus.shown {
                    ToneDotValue(
                        text: shown.statusLine,
                        tone: shown.statusTone
                    )
                }
            }
            .pressing(.opens { path.append(.sync) })
        }
        else {
            MacRow("Sync") {
                HStack {
                    Text("Not synced")
                        .foregroundStyle(.secondary)
                    MacButton("Set up a cloud home…", kind: .primary) {
                        showSyncSetup = true
                    }
                }
            }
            .pressing(.opens { path.append(.sync) })
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
/// last finished, then where the library is stored and its restore code. On an
/// S3 library the page replaces the library's key.
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
    /// The S3 key sheet and the line after a save, which go when the page
    /// closes.
    @State
    private var keyReplacement: S3KeyReplacement

    init(
        restoreCode: RestoreCodeReveal,
        keyReplacement: S3KeyReplacement = S3KeyReplacement(),
        onResetDevice: @escaping () -> Void,
        onRestoreFromCode: @escaping () -> Void,
        onCheckForUpdates: @escaping () -> Void,
        onRemoveLibrary: @escaping () -> Void
    ) {
        self.restoreCode = restoreCode
        _keyReplacement = State(initialValue: keyReplacement)
        self.onResetDevice = onResetDevice
        self.onRestoreFromCode = onRestoreFromCode
        self.onCheckForUpdates = onCheckForUpdates
        self.onRemoveLibrary = onRemoveLibrary
    }

    var body: some View {
        VStack(spacing: 0) {
            MacPageHeading(
                title: Text(verbatim: String(localized: "Sync")),
                back: .init(
                    title: String(localized: "Library"),
                    action: { dismiss() }
                )
            )
            form
        }
        .navigationBarBackButtonHidden(true)
        .sheet(
            isPresented: Binding(
                get: { keyReplacement.isOpen },
                set: { open in if !open { keyReplacement.cancel() } }
            )
        ) {
            S3KeySheet(replacement: keyReplacement)
        }
    }

    private var form: some View {
        Form {
            MacSection {
                UndoneSetupLine()
                MacSyncStatus(
                    location: configStore.storage,
                    actions: SyncFixActions(
                        onResetDevice: onResetDevice,
                        onRestoreFromCode: onRestoreFromCode,
                        onCheckForUpdates: onCheckForUpdates,
                        onRemoveLibrary: onRemoveLibrary,
                        onEnterAccessKey: { keyReplacement.open() }
                    )
                )
            }
            if let location = configStore.storage {
                MacCloudHomeContainer(
                    location: location,
                    keyReplacement: keyReplacement
                )
                LibraryRestoreCodeSection(
                    reveal: restoreCode,
                    location: location
                )
            }
        }
        .formStyle(.settings)
    }
}

/// The People and devices page, which has its own header and back button
/// for the pages inside it, so the stack's is hidden.

#if DEBUG
    // periphery:ignore - called by baeShots
    /// The Sync page with actions that do nothing, for a shot, its S3 key
    /// sheet and saved line as `keyReplacement` holds them.
    @MainActor
    func syncSettingsPageWithInertActions(
        restoreCode: RestoreCodeReveal,
        keyReplacement: S3KeyReplacement = S3KeyReplacement()
    ) -> some View {
        SyncSettingsPage(
            restoreCode: restoreCode,
            keyReplacement: keyReplacement,
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
        MacRestoreCode(
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
