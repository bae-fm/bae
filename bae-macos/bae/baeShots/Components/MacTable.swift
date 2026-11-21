import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTable`: three releases, the second uploading; the same with the
    /// second selected; the same in the room the inspector leaves the table,
    /// which keeps every column at its width and scrolls; and three rows
    /// whose releases have not loaded yet.
    static let macTableScenes: [ShotScene] = [
        storageTable(
            "mac.component.table.rows",
            size: CGSize(width: fullTableWidth, height: 150),
            rows: threeReleases(),
            uploading: "rel-shot-2"
        ),
        storageTable(
            "mac.component.table.selected",
            size: CGSize(width: fullTableWidth, height: 150),
            rows: threeReleases(),
            selected: ["rel-shot-2"],
            uploading: "rel-shot-2"
        ),
        storageTable(
            "mac.component.table.beside-the-inspector",
            size: CGSize(width: 560, height: 150),
            rows: threeReleases(),
            uploading: "rel-shot-2"
        ),
        storageTable(
            "mac.component.table.loading",
            size: CGSize(width: fullTableWidth, height: 150),
            rows: threeReleases(),
            unloaded: true
        ),
    ]

    /// Every column at the width the table starts with.
    private static let everyColumn: StorageTableLayout = {
        let layout = StorageTableLayout(
            holding: StorageTableColumn.allCases.map {
                StorageTableLayout.Column(
                    column: $0,
                    width: $0.width,
                    hidden: false
                )
            }
        )
        guard let layout else {
            preconditionFailure("the shot's layout holds every column once")
        }
        return layout
    }()

    private static let releaseSort = BridgeStorageSort(
        field: .albumTitle,
        direction: .ascending
    )

    /// The real table with `rows` loaded, drawn at `size` over its columns.
    /// With `unloaded` the rows are counted and no page of them has landed,
    /// so each draws its placeholder.
    private static func storageTable(
        _ id: String,
        size: CGSize,
        rows: [BridgeStorageRow],
        selected: Set<String> = [],
        unloaded: Bool = false,
        uploading: String? = nil
    ) -> ShotScene {
        ShotScene(id: id, size: size) {
            let fixture = StorageManagerPreviewFixture(
                rows: rows,
                downloads: [],
                outputs: [],
                uploads: [],
                unloaded: unloaded
            )
            let runner = StorageActionRunner(
                releaseEditor: .stub(),
                sync: .stub(),
                downloads: .stub(),
                outputs: .stub(),
                configStore: PreviewData.configStore(),
                uiStore: fixture.uiStore,
                openSaveAsSettings: {}
            )
            let table = ShotStorageList(store: fixture.storageManagerStore) {
                _,
                list in
                MacTable(
                    list: list,
                    selection: .constant(selected),
                    sort: .constant(releaseSort),
                    inspectorPresented: .constant(false),
                    layout: .constant(everyColumn),
                    runner: runner
                )
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Theme.surfaceElevated)
            return ShotStage(
                fixture.environment(
                    of: table,
                    outbox: uploading.map(uploadingOutbox)
                )
            ) {
                fixture.storageManagerStore.list != nil
            }
        }
    }

    private static let fullTableWidth: CGFloat = 1_080
}
