import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Sync page's boards (`mac.settings.sync.*`) that show what sync
    /// waits on or why its cloud home isn't shown: the pending records as
    /// rows, the setup that ended undone, a cloud home that couldn't be
    /// read, and the restore code shown. Each shows the whole Settings
    /// window, as the boards draw it: the sidebar with Library chosen, and
    /// the page over the Library pane.
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
                    ),
                    pending: [
                        BridgePendingRecord(
                            subject: BridgeMessage(
                                key: "core.pending.subject.release_file",
                                args: [
                                    BridgeMessageArg(
                                        name: "file",
                                        value: "03 Second Floor.flac"
                                    ),
                                    BridgeMessageArg(
                                        name: "album",
                                        value: "Paper Lanterns"
                                    ),
                                ]
                            ),
                            reason: BridgePendingReason(
                                key: "core.pending.reason.file_changed",
                                detail:
                                    "the file changed after it was imported",
                                causes: []
                            ),
                            action: .restoreFile,
                            actionKey: "core.pending.action.restore_file",
                            sendsFiles: true
                        ),
                        BridgePendingRecord(
                            subject: BridgeMessage(
                                key: "core.pending.subject.cover",
                                args: [
                                    BridgeMessageArg(
                                        name: "album",
                                        value: "Low Tide Hours"
                                    )
                                ]
                            ),
                            reason: BridgePendingReason(
                                key: "core.pending.reason.storage",
                                detail: "request failed: connection refused",
                                causes: []
                            ),
                            action: .retryOperation(id: 7),
                            actionKey: "core.pending.action.retry",
                            sendsFiles: true
                        ),
                    ]
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
                    shown: .previewNotSynced,
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
            id: "mac.settings.sync.cloud-home-unreadable",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                syncSettingsWindow(
                    LibrarySettingsTabPreviewFixture(storage: nil),
                    shown: .previewSynced,
                    storageReadError: DisplayError(
                        line: "Could not read the cloud home.",
                        detail: "database error: no such record"
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
                    shown: .previewSynced,
                    restoreCode: .shown(code: PreviewData.restoreCodeGrouped)
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
    pending: [BridgePendingRecord] = [],
    undoneSetup: BridgeUndoneStorageSetup? = nil,
    storageReadError: DisplayError? = nil,
    restoreCode: RestoreCodeReveal.State = .masked(
        code: PreviewData.restoreCodeGrouped
    )
) -> some View {
    SettingsWindowShot(
        shown: shown,
        pending: pending,
        undoneSetup: undoneSetup,
        page: syncSettingsPagePreview(
            fixture,
            shown: shown,
            pending: pending,
            undoneSetup: undoneSetup,
            storageReadError: storageReadError,
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
        people: PreviewData.members(reaching: PreviewData.previewStorage),
        selfIsAdmin: true,
        invites: invites
    )
    return OverWindow.alert(
        alert,
        over: SettingsWindowShot(
            shown: .previewSynced,
            pending: [],
            undoneSetup: nil,
            page: PeopleAndDevicesPage(onBack: {}, subpage: subpage)
                .environment(
                    PreviewData.previewSync(storage: PreviewData.previewStorage)
                )
                .environment(
                    PreviewData.connectedConfigStore(
                        storage: PreviewData.previewStorage
                    )
                )
                .environment(membership)
        )
    )
}
