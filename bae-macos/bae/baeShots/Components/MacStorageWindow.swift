import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStorageWindow`: the release list; the inspector open on a
    /// selected release; the Cloud filter and a search; a search that
    /// matches nothing; and, with no library open, the window the app shows
    /// in its place.
    static let macStorageWindowScenes: [ShotScene] = [
        ShotScene(
            id: "mac.component.storage-window.release-list",
            size: storageWindowSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageWindow()
        },
        ShotScene(
            id: "mac.component.storage-window.inspector-open",
            size: storageWindowSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageWindow(selected: "rel-row-3", inspector: true)
        },
        ShotScene(
            id: "mac.component.storage-window.filtered",
            size: storageWindowSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageWindow(
                rows: [glassHarborRow(1)],
                filter: .remote,
                search: "harbor"
            )
        },
        ShotScene(
            id: "mac.component.storage-window.no-matches",
            size: storageWindowSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageWindow(rows: [], search: "zzz")
        },
        ShotScene(
            id: "mac.component.storage-window.no-library",
            size: storageWindowSize,
            frame: .window(title: "Storage Manager")
        ) {
            ShotStage(StorageManagerNoLibrary())
        },
    ]

    private static let storageWindowSize = CGSize(width: 900, height: 400)

    /// The Storage Manager window over the board's twelve releases, as the
    /// app opens it, or over `rows` for a search.
    @MainActor
    private static func storageWindow(
        rows: [BridgeStorageRow] = boardStorageRows,
        filter: BridgeStorageFilter = .all,
        search: String = "",
        selected: String? = nil,
        inspector: Bool = false
    ) -> ShotStage {
        let fixture = StorageManagerPreviewFixture(
            rows: rows,
            search: search,
            selectedReleaseId: selected,
            inspectorPresented: inspector
        )
        let window = MacStorageWindow(
            initialFilter: filter,
            initialSearch: search,
            initialSelection: fixture.initialSelection,
            initialInspectorPresented: inspector
        )
        .frame(
            width: storageWindowSize.width,
            height: storageWindowSize.height
        )
        return ShotStage(
            fixture.environment(of: window)
        ) {
            fixture.storageManagerStore.list?.count != nil
        }
    }
}
