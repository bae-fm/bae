import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of the main window's chrome: the Activity panel and
    /// its transfer lines, the Activity button, the section tabs and the
    /// title bar.
    static let chromeComponentScenes: [ShotScene] =
        activityScenes + transferLineScenes + activityButtonScenes
        + sectionTabsScenes + titleBarScenes

    // MARK: - MacActivity

    /// `MacActivity` at the width the 360pt panel's 14pt inset leaves it.
    private static let activityScenes: [ShotScene] = [
        activityPanel("mac.component.activity.up-to-date", height: 90),
        activityPanel(
            "mac.component.activity.transfers",
            height: 330,
            activity: BridgeSyncActivity(
                moving: true,
                held: false,
                failure: nil,
                syncing: true,
                uploads: BridgeUploadQueueActivity(
                    batch: BridgeTransferBatch(done: 1, total: 2),
                    wait: nil
                ),
                pins: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 1, total: 3),
                    wait: nil,
                    paused: false
                ),
                movingToThisDevice: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 0, total: 1),
                    wait: nil,
                    paused: false
                )
            ),
            bars: ["0": 0.43, "1": 0.3, "2": 0.2]
        ),
        activityPanel(
            "mac.component.activity.queues-held",
            height: 300,
            activity: BridgeSyncActivity(
                moving: false,
                held: true,
                failure: nil,
                syncing: false,
                uploads: BridgeUploadQueueActivity(
                    batch: BridgeTransferBatch(done: 1, total: 2),
                    wait: .metered
                ),
                pins: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 1, total: 3),
                    wait: .paused,
                    paused: true
                ),
                movingToThisDevice: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 0, total: 1),
                    wait: .paused,
                    paused: true
                )
            )
        ),
        activityPanel(
            "mac.component.activity.status-unknown",
            height: 130,
            readError: ShotReadError(
                text: "bae couldn’t read the sync status."
            )
        ),
        activityPanel(
            "mac.component.activity.sync-needs-you",
            height: 260,
            activity: BridgeSyncActivity(
                moving: false,
                held: false,
                failure: syncRefused,
                syncing: true,
                uploads: nil,
                pins: nil,
                movingToThisDevice: nil
            )
        ),
    ]

    /// Storage turning this Mac's sign-in down, which sync reports as its
    /// failure.
    private static let syncRefused = BridgeError.Diagnostic(
        category: .storage(failure: .authentication),
        detail: "Dropbox answered 401: expired_access_token"
    )

    /// The store the Activity panel and button read: `activity`, the bars of
    /// its lines at `bars`, and the failure to read it when `readError`.
    @MainActor
    private static func activityStore(
        _ activity: BridgeSyncActivity?,
        bars: [String: Double] = [:],
        readError: (any Error)? = nil
    ) -> SyncActivityStore {
        let store = SyncActivityStore(
            activity: activity,
            bars: .activityBars(holding: bars)
        )
        if let readError {
            store.fail(readError)
        }
        return store
    }

    private static func activityPanel(
        _ id: String,
        height: CGFloat,
        activity: BridgeSyncActivity? = nil,
        bars: [String: Double] = [:],
        readError: (any Error)? = nil,
        app: [AppProblem] = []
    ) -> ShotScene {
        component(id, width: 332, height: height) {
            let store = activityStore(
                activity,
                bars: bars,
                readError: readError
            )
            MacActivity(
                problems: ActivityProblems(
                    app: app,
                    activity: activity,
                    error: store.error,
                    subsonic: .off,
                    dismissed: []
                ),
                app: app,
                dismissed: .constant([]),
                onOpenSettings: {},
                onOpenStorageManager: {},
                subsonic: .off,
                onOpenSubsonicSettings: {}
            )
            .environment(store)
            .transferLineStores()
        }
    }

    // MARK: - MacTransferLine

    /// `MacTransferLine` in the Activity panel's 332pt column.
    private static let transferLineScenes: [ShotScene] = [
        transferLine(
            "mac.component.transfer-line.syncing",
            title: "Syncing changes",
            count: nil,
            queue: nil,
            share: nil
        ),
        transferLine(
            "mac.component.transfer-line.uploading",
            title: "Uploading",
            count: "1 of 3 releases",
            queue: .init(.uploads, paused: false),
            share: 0.4
        ),
        transferLine(
            "mac.component.transfer-line.waiting",
            title: "Uploading",
            count: "Waiting for Wi-Fi",
            queue: .init(.uploads, paused: false),
            share: 0.4
        ),
        transferLine(
            "mac.component.transfer-line.paused",
            title: "Pinning",
            count: "Paused",
            queue: .init(.downloads, paused: true),
            share: 0.3
        ),
        transferLine(
            "mac.component.transfer-line.moving-to-this-mac",
            title: "Moving to This Mac",
            count: "0 of 1 release",
            queue: .init(.downloads, paused: false),
            share: 0.2
        ),
    ]

    private static func transferLine(
        _ id: String,
        title: String,
        count: String?,
        queue: MacTransferLine.Queue?,
        share: Double?
    ) -> ShotScene {
        component(id, width: 332, height: 80) {
            MacTransferLine(
                title: title,
                count: count,
                queue: queue,
                bar: share.map {
                    LatestStream.activityBars(holding: ["0": $0])
                        .source(for: "0")
                }
            )
            .transferLineStores()
        }
    }

    /// What the title bar's activity button reads while bytes move, while a
    /// problem is listed and while a queue is held: the button's scenes and
    /// the title bar's draw the same three.
    private static let movingActivity = BridgeSyncActivity(
        moving: true,
        held: false,
        failure: nil,
        syncing: true,
        uploads: nil,
        pins: nil,
        movingToThisDevice: nil
    )

    private static let attentionActivity = BridgeSyncActivity(
        moving: false,
        held: false,
        failure: syncRefused,
        syncing: false,
        uploads: nil,
        pins: nil,
        movingToThisDevice: nil
    )

    private static let pausedActivity = BridgeSyncActivity(
        moving: false,
        held: true,
        failure: nil,
        syncing: false,
        uploads: nil,
        pins: BridgeDownloadQueueActivity(
            batch: BridgeTransferBatch(done: 1, total: 3),
            wait: .paused,
            paused: true
        ),
        movingToThisDevice: nil
    )

    // MARK: - MacActivityButton

    /// `MacActivityButton` as the title bar shows it while bytes move, while
    /// a problem is listed and while a queue is held.
    private static let activityButtonScenes: [ShotScene] = [
        activityButton(
            "mac.component.activity-button.moving",
            movingActivity
        ),
        activityButton(
            "mac.component.activity-button.attention",
            attentionActivity
        ),
        activityButton(
            "mac.component.activity-button.paused",
            pausedActivity
        ),
    ]

    private static func activityButton(
        _ id: String,
        _ activity: BridgeSyncActivity
    ) -> ShotScene {
        component(id, width: 64, height: 60) {
            MacActivityButton()
                .environment(activityStore(activity))
                .environment(SubsonicStore())
                .environment(SettingsNavigation())
        }
    }

    // MARK: - MacSectionTabs

    /// `MacSectionTabs` on each section.
    private static let sectionTabsScenes: [ShotScene] = [
        component("mac.component.section-tabs.library", width: 224, height: 60)
        {
            MacSectionTabs(selection: .library, onSelect: { _ in })
        },
        component("mac.component.section-tabs.import", width: 224, height: 60) {
            MacSectionTabs(selection: .importing, onSelect: { _ in })
        },
    ]

    // MARK: - MacTitleBar

    /// `MacTitleBar` over the library, 1100pt wide as the main window's
    /// preview draws it.
    private static let titleBarScenes: [ShotScene] = [
        titleBar("mac.component.title-bar.library", section: .library),
        titleBar("mac.component.title-bar.import", section: .importing),
        titleBar(
            "mac.component.title-bar.moving",
            section: .library,
            activity: movingActivity
        ),
        titleBar(
            "mac.component.title-bar.attention",
            section: .library,
            activity: attentionActivity
        ),
        titleBar(
            "mac.component.title-bar.paused",
            section: .library,
            activity: pausedActivity
        ),
    ]

    /// A store whose main window shows `section`.
    @MainActor
    static func storeShowing(_ section: MainSection) -> UiStore {
        let store = UiStore()
        store.activeSection = section
        return store
    }

    private static func titleBar(
        _ id: String,
        section: MainSection,
        activity: BridgeSyncActivity? = nil
    ) -> ShotScene {
        component(id, width: 1_100, height: 56) {
            let uiStore = storeShowing(section)
            let backing = boardLibraryBacking(uiStore: uiStore)
            MacTitleBar(
                searchText: .constant(""),
                onSearchFieldFrame: { _ in }
            )
            .environment(uiStore)
            .environment(LibraryProjectionStore(library: backing.library))
            .environment(activityStore(activity))
            .environment(SubsonicStore())
            .environment(SettingsNavigation())
            // Ana, whose picture ends the bar.
            .environment(
                MembershipStore.stub(
                    people: PreviewData.members(
                        reaching: PreviewData.previewDropboxStorage
                    ),
                    selfIsAdmin: true
                )
            )
            .environment(PreviewData.syncStatus(hasCloudHome: true))
        }
    }
}

/// The text of an error a shot's store failed to read.
private struct ShotReadError: LocalizedError {
    let text: String

    var errorDescription: String? { text }
}

extension View {
    /// The stores a transfer line's pause button reads.
    fileprivate func transferLineStores() -> some View {
        environment(Sync.stub())
            .environment(Downloads.stub())
            .environment(Outputs.stub())
            .environment(UiStore())
    }
}
