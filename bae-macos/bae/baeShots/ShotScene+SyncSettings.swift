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
            frame: .window(title: "Settings")
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
            frame: .window(title: "Settings")
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
            frame: .window(title: "Settings")
        ) {
            ShotStage(
                syncSettingsWindow(
                    LibrarySettingsTabPreviewFixture(storage: nil),
                    shown: .previewSynced,
                    storageReadError: DisplayError(
                        line: "Couldn’t read the cloud home.",
                        detail: "database error: no such record"
                    )
                )
            )
        },
        ShotScene(
            id: "mac.settings.sync.code-shown",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings")
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
    SyncSettingsWindow(
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

private struct SyncSettingsWindow<Page: View>: View {
    let shown: BridgeSyncShown
    let pending: [BridgePendingRecord]
    let undoneSetup: BridgeUndoneStorageSetup?
    let page: Page

    /// The pane the window is on, as the person left it.
    @State
    private var selection = SettingsTab.library

    var body: some View {
        NavigationSplitView(columnVisibility: .constant(.all)) {
            SettingsSidebar(selection: $selection)
                .navigationSplitViewColumnWidth(190)
        } detail: {
            page.frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .toolbar(removing: .sidebarToggle)
        // The size SettingsView gives its window.
        .frame(width: 690, height: 600)
        .environment(DownloadStore())
        .environment(OutboxStore(uploadsPaused: false))
        .environment(SubsonicStore())
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
    }
}
