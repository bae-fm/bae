import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Sync page's boards (`mac.settings.sync.*`) that show what sync
    /// waits on or why its cloud home isn't shown: sync waiting, the setup
    /// that ended undone, a cloud home that couldn't be read, and the
    /// restore code shown. Each shows the whole Settings window, as the
    /// boards draw it: the sidebar with Library chosen, and the page over the
    /// Library pane.
    static let syncSettingsScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.sync.pending",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                syncSettingsWindow(
                    LibrarySettingsTabPreviewFixture(
                        storage: PreviewData.previewDropboxStorage
                    ),
                    shown: BridgeSyncShown(
                        status: .waitingToSync,
                        statusKey: "core.sync.status.waiting_to_sync",
                        tone: .neutral,
                        cause: nil,
                        fix: nil,
                        lastSynced: Date(timeIntervalSinceNow: -2 * 3_600)
                    )
                )
            )
        },
        ShotScene(
            id: "mac.settings.sync.setup-undone",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                syncSettingsWindow(
                    LibrarySettingsTabPreviewFixture(storage: nil),
                    shown: .fixtureNotSynced,
                    undoneSetup: BridgeUndoneStorageSetup(
                        operation: 1,
                        error: BridgeError.Diagnostic(
                            category: .storageSetupUndone,
                            detail: "the setup was stopped partway"
                        )
                    )
                )
            )
        },
        ShotScene(
            id: "mac.settings.sync.code-shown",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                syncSettingsWindow(
                    LibrarySettingsTabPreviewFixture(
                        storage: PreviewData.previewDropboxStorage
                    ),
                    shown: .fixtureSynced,
                    restoreCode: .shown(PreviewData.restoreCodeShare)
                )
            )
        },
    ]
}

/// The Settings window around the Sync page of `fixture`'s library: the
/// sidebar as `SettingsView` lists it, and the page `syncSettingsPagePreview`
/// builds beside it.
@MainActor
private func syncSettingsWindow(
    _ fixture: LibrarySettingsTabPreviewFixture,
    shown: BridgeSyncShown,
    undoneSetup: BridgeUndoneStorageSetup? = nil,
    restoreCode: RestoreCodeReveal.State = .masked(
        PreviewData.restoreCodeShare
    )
) -> some View {
    SettingsWindowShot(
        shown: shown,
        undoneSetup: undoneSetup,
        page: syncSettingsPagePreview(
            fixture,
            shown: shown,
            undoneSetup: undoneSetup,
            restoreCode: restoreCode
        )
    )
}

/// The Settings window on the Library pane, showing People and devices
/// `subpage` for an admin with `invites` open, with `alert` laid over it.
@MainActor
func peopleSettingsWindow(
    subpage: PeopleAndDevicesPage.Subpage,
    invites: [BridgeInvite] = [],
    alert: (MacAlertWords, [MacAlertButton])
) -> some View {
    let membership = MembershipStore.stub(
        people: PreviewData.members(
            reaching: PreviewData.previewDropboxStorage
        ),
        selfIsAdmin: true,
        invites: invites
    )
    return OverWindow.alert(
        alert,
        over: SettingsWindowShot(
            shown: .fixtureSynced,
            undoneSetup: nil,
            page: PeopleAndDevicesPage(onBack: {}, subpage: subpage)
                .environment(
                    PreviewData.previewSync()
                )
                .environment(
                    PreviewData.connectedConfigStore(
                        storage: PreviewData.previewDropboxStorage
                    )
                )
                .environment(membership)
        )
    )
}
