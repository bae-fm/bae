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
                    ]
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
            return ShotStage(ActivityPanelShot(store: store, app: []))
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
            return ShotStage(ActivityPanelShot(store: store, app: []))
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
            needsAttention: failure != nil,
            failure: failure,
            attention: [],
            syncing: false,
            uploads: BridgeUploadQueueActivity(
                batch: BridgeTransferBatch(done: uploadsDone, total: 3),
                wait: nil
            ),
            pins: nil,
            makingLocal: nil,
            retryOffered: false
        )
    }
}

/// The popover's form as the Activity button shows it.
private struct ActivityPanelShot: View {
    let store: SyncActivityStore
    let app: [AppProblem]
    @State
    private var dismissed: Set<ActivityProblemID> = []

    var body: some View {
        Form {
            ActivityList(
                problems: ActivityProblems(
                    app: app,
                    activity: store.activity,
                    error: store.error,
                    dismissed: dismissed
                ),
                app: app,
                dismissed: $dismissed,
                onOpenSettings: {}
            )
        }
        .formStyle(.grouped)
        .environment(store)
    }
}
