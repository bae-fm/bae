import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStorageWindow`: the release list; the inspector, the preview's
    /// stand-in, open on a selected release; the Cloud filter and a search; a search that
    /// matches nothing; and, with no library open, the window the app shows
    /// in its place.
    static let macStorageWindowScenes: [ShotScene] = [
        ShotScene(
            id: "mac.component.storage-window.release-list",
            size: storageWindowSize,
            frame: .window(title: "Storage Manager", titleBar: .themed)
        ) {
            storageWindow()
        },
        ShotScene(
            id: "mac.component.storage-window.inspector-open",
            size: storageWindowSize,
            frame: .window(title: "Storage Manager", titleBar: .themed)
        ) {
            storageWindow(selected: "rel-shot-2", inspector: true)
        },
        ShotScene(
            id: "mac.component.storage-window.filtered",
            size: storageWindowSize,
            frame: .window(title: "Storage Manager", titleBar: .themed)
        ) {
            storageWindow(
                rows: threeReleases(),
                filter: .remote,
                search: "harbor"
            )
        },
        ShotScene(
            id: "mac.component.storage-window.no-matches",
            size: storageWindowSize,
            frame: .window(title: "Storage Manager", titleBar: .themed)
        ) {
            storageWindow(rows: [], search: "zzz")
        },
        ShotScene(
            id: "mac.component.storage-window.no-library",
            size: storageWindowSize,
            frame: .window(title: "Storage Manager", titleBar: .themed)
        ) {
            ShotStage(StorageManagerNoLibrary())
        },
    ]

    private static let storageWindowSize = CGSize(width: 1100, height: 400)

    /// The Storage Manager window over the preview's three releases, with
    /// no transfer running, or over `rows` for a search. Its inspector is the
    /// preview's stand-in, since the window only hosts it.
    @MainActor
    private static func storageWindow(
        rows: [BridgeStorageRow] = threeReleases(),
        filter: BridgeStorageFilter = .all,
        search: String = "",
        selected: String? = nil,
        inspector: Bool = false
    ) -> ShotStage {
        let fixture = StorageManagerPreviewFixture(
            rows: rows,
            search: search,
            selectedReleaseId: selected,
            inspectorPresented: inspector,
            downloads: [],
            outputs: [],
            queues: BridgeStorageQueues(
                uploads: nil,
                downloads: nil,
                outputs: nil
            )
        )
        let window = MacStorageWindow(
            initialFilter: filter,
            initialSearch: search,
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: inspector
        ) {
            // The preview's stand-in for the inspector the window hosts, at
            // the top of the inspector's 440 pt column.
            PreviewSlot("Inspector")
                .frame(width: 440)
                .frame(maxHeight: .infinity, alignment: .top)
        }
        .frame(
            width: storageWindowSize.width,
            height: storageWindowSize.height
        )
        let images = coverStore()
        return ShotStage(
            fixture.environment(
                of: window,
                outputs: OutputStore(),
                outbox: uploadingOutbox("rel-shot-2"),
                images: images
            ),
            images: images
        ) {
            fixture.storageManagerStore.list?.count != nil
        }
    }
}
