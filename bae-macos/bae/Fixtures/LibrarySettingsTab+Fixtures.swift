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

    init(
        storage: BridgeStorageLocation?,
        storageAccess: BridgeMemberAccess? = nil
    ) {
        configStore =
            storage.map {
                PreviewData.connectedConfigStore(
                    storage: $0,
                    storageAccess: storageAccess
                )
            }
            ?? PreviewData.configStore()
        membership = MembershipStore.stub(
            people: storage.map { PreviewData.members(reaching: $0) }
                ?? PreviewData.noPeople,
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
    .environment(PreviewData.previewSync())
    #if BAE_OAUTH_PROVIDERS
        .environment(CloudSyncSetup())
    #endif
    .environment(fixture.configStore)
    .environment(
        SyncStatusStore(
            snapshot: BridgeSyncStatusSnapshot(
                shown: shown,
                setupRunning: false,
                hasCloudHome: true,
                undoneSetup: nil
            )
        )
    )
    .environment(fixture.membership)
    .environment(UiStore())
    .environment(fixture.libraryName)
}

/// The Sync page over `fixture`'s library, its sync as `shown` says, with
/// a storage setup that ended undone, when given, its restore code masked
/// unless `restoreCode` says otherwise, and the S3 key sheet and saved line as
/// `keyReplacement` holds them.
@MainActor
func syncSettingsPagePreview(
    _ fixture: LibrarySettingsTabPreviewFixture,
    shown: BridgeSyncShown,
    undoneSetup: BridgeUndoneStorageSetup? = nil,
    restoreCode: RestoreCodeReveal.State = .masked(
        PreviewData.restoreCodeShare
    ),
    keyReplacement: S3KeyReplacement = S3KeyReplacement()
) -> some View {
    let sync = PreviewData.previewSync()
    let status = SyncStatusStore(
        snapshot: BridgeSyncStatusSnapshot(
            shown: shown,
            setupRunning: false,
            hasCloudHome: true,
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
                    keyReplacement: keyReplacement
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
