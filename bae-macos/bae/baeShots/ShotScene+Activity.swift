import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Activity panel's popover over the problems it lists, each beside
    /// its canvas board.
    static let activityScenes: [ShotScene] = [
        // The app's own problems over an upload: the canvas's ErrMacNotices.
        ShotScene(
            id: "activity-problems",
            size: CGSize(width: 360, height: 480)
        ) {
            let store = SyncActivityStore(
                activity: ActivityShot.activity(uploadsDone: 0)
            )
            return ShotStage(
                ActivityPanelShot(
                    store: store,
                    app: LaunchProblems(
                        signInSettings: DisplayError(
                            line: "The sign-in settings file is missing.",
                            detail: "OAuthConfig: missing client id"
                        )
                    )
                    .problems + [
                        LoadError(
                            .libraryOpen,
                            DisplayError(
                                line: "The library couldn’t be opened.",
                                detail: "database is locked"
                            )
                        )
                        .problem
                    ],
                    subsonic: .off
                )
            )
        },
        // bae couldn't read sync's state: SyncMacFailed.
        ShotScene(
            id: "activity-failed",
            size: CGSize(width: 360, height: 480)
        ) {
            let store = SyncActivityStore()
            store.fail(
                BridgeError.Diagnostic(
                    category: .internal,
                    detail: "dns error: failed to lookup address"
                )
            )
            return ShotStage(
                ActivityPanelShot(store: store, app: [], subsonic: .off)
            )
        },
        // Sync's own failure with Open sync settings: SyncMacAttention.
        ShotScene(
            id: "activity-attention",
            size: CGSize(width: 360, height: 480)
        ) {
            let store = SyncActivityStore(
                activity: ActivityShot.activity(
                    uploadsDone: 0,
                    failure: BridgeError.Diagnostic(
                        category: .credentials,
                        detail: "dropbox: invalid_access_token"
                    )
                )
            )
            return ShotStage(
                ActivityPanelShot(store: store, app: [], subsonic: .off)
            )
        },
        // Every line moving: SyncMacMoving.
        ShotScene(
            id: "activity-transfers",
            size: CGSize(width: 360, height: 480)
        ) {
            let store = SyncActivityStore(
                activity: ActivityShot.transfers(),
                bars: .activityBars(
                    holding: [
                        "uploads": 0.43, "pins": 0.3, "making_local": 0.2,
                    ]
                )
            )
            return ShotStage(
                ActivityPanelShot(store: store, app: [], subsonic: .off)
            )
        },
        // Uploads paused and downloads waiting for network, each in the
        // count's place: SyncMacPaused.
        ShotScene(
            id: "activity-transfers-held",
            size: CGSize(width: 360, height: 480)
        ) {
            let store = SyncActivityStore(
                activity: ActivityShot.transfers(
                    uploadsWait: .paused,
                    downloadsWait: .offline
                ),
                bars: .activityBars(
                    holding: ["uploads": 0.3, "pins": 0, "making_local": 0]
                )
            )
            return ShotStage(
                ActivityPanelShot(store: store, app: [], subsonic: .off)
            )
        },
        // The title bar's button while both queues are paused and nothing
        // moves: the pause symbol of SyncMacPaused.
        ShotScene(
            id: "activity-button-held",
            size: CGSize(width: 120, height: 56)
        ) {
            let store = SyncActivityStore(
                activity: ActivityShot.transfers(
                    uploadsWait: .paused,
                    downloadsWait: .paused
                )
            )
            return ShotStage(
                ActivityButton()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .environment(store)
                    .environment(SubsonicStore())
                    .environment(SettingsNavigation())
            )
        },
        // The server switched on and not running, alone in Needs attention:
        // the canvas's SetMacSubsonicActivity.
        ShotScene(
            id: "activity-subsonic",
            size: CGSize(width: 360, height: 200)
        ) {
            ShotStage(
                ActivityPanelShot(
                    store: SyncActivityStore(activity: nil),
                    app: [],
                    subsonic: .notRunning(
                        causes: [.missingUsername, .missingPassword],
                        tone: .warning
                    )
                )
            )
        },
    ]
}

/// The activities the Activity shots show.
private enum ActivityShot {
    /// Releases uploading, three in the batch, `uploadsDone` of them done.
    static func activity(
        uploadsDone: UInt32,
        failure: BridgeError? = nil
    ) -> BridgeSyncActivity {
        BridgeSyncActivity(
            moving: true,
            held: false,
            needsAttention: failure != nil,
            failure: failure,
            attention: [],
            syncing: false,
            uploads: BridgeUploadQueueActivity(
                batch: BridgeTransferBatch(done: uploadsDone, total: 3),
                wait: nil,
                paused: false
            ),
            pins: nil,
            makingLocal: nil
        )
    }

    /// Syncing, and an upload, a pin and a make-local, each in the middle
    /// of its batch: moving, or held with nothing moving while the uploads
    /// wait.
    static func transfers(
        uploadsWait: BridgeUploadQueueWait? = nil,
        downloadsWait: BridgeQueueWait? = nil
    ) -> BridgeSyncActivity {
        let uploadsPaused = uploadsWait == .paused
        let downloadsPaused = downloadsWait == .paused
        return BridgeSyncActivity(
            moving: uploadsWait == nil,
            held: uploadsWait != nil,
            needsAttention: false,
            failure: nil,
            attention: [],
            syncing: true,
            uploads: BridgeUploadQueueActivity(
                batch: BridgeTransferBatch(done: 1, total: 2),
                wait: uploadsWait,
                paused: uploadsPaused
            ),
            pins: BridgeDownloadQueueActivity(
                batch: BridgeTransferBatch(done: 1, total: 3),
                wait: downloadsWait,
                paused: downloadsPaused
            ),
            makingLocal: BridgeDownloadQueueActivity(
                batch: BridgeTransferBatch(done: 0, total: 1),
                wait: downloadsWait,
                paused: downloadsPaused
            )
        )
    }
}

/// The popover's form as the Activity button shows it.
private struct ActivityPanelShot: View {
    let store: SyncActivityStore
    let app: [AppProblem]
    let subsonic: BridgeSubsonicStatus
    @State
    private var dismissed: Set<ActivityProblemID> = []

    var body: some View {
        Form {
            ActivityList(
                problems: ActivityProblems(
                    app: app,
                    activity: store.activity,
                    error: store.error,
                    subsonic: subsonic,
                    dismissed: dismissed
                ),
                app: app,
                dismissed: $dismissed,
                onOpenSettings: {},
                onOpenStorageManager: {},
                subsonic: subsonic,
                onOpenSubsonicSettings: {}
            )
        }
        .formStyle(.grouped)
        .environment(store)
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(UiStore())
    }
}
