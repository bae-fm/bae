import BaeKit
import Foundation
import SwiftUI

@testable import bae

// MARK: - Shot staging

/// The Library tab's shot stores over a library whose storage is
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
    shown: BridgeSyncShown = .fixtureSynced
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
    let sync = PreviewData.previewSync(
        storage: fixture.configStore.storage
    )
    let status = SyncStatusStore(
        snapshot: BridgeSyncStatusSnapshot(
            shown: shown,
            pending: pending,
            setupRunning: false,
            undoneSetup: undoneSetup
        )
    )
    let uiStore = UiStore()
    // The page as the Library pane opens it: pushed over the pane, with
    // the pane's name on its back button. The page takes the stores
    // itself: those set outside a pushed page didn't reach it.
    return NavigationStack(path: .constant([0])) {
        Form {}
            .formStyle(.grouped)
            .navigationTitle("Library")
            .navigationDestination(for: Int.self) { _ in
                syncSettingsPageWithInertActions(
                    restoreCode: RestoreCodeReveal(
                        previewState: restoreCode
                    ),
                    storageReadError: storageReadError
                )
                .environment(sync)
                #if BAE_OAUTH_PROVIDERS
                    .environment(CloudSyncSetup())
                #endif
                .environment(fixture.configStore)
                .environment(status)
                .environment(uiStore)
            }
    }
}
