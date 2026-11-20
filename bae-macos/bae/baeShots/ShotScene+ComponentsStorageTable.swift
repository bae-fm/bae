import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of the Storage Manager: its table, a row of the
    /// table, the footer, the toolbar and the window.
    static let storageTableComponentScenes: [ShotScene] =
        tableScenes + storageFooterScenes
        + storageToolbarScenes + storageWindowScenes

    // MARK: Rows

    private static func glassHarborRow(_ number: Int) -> BridgeStorageRow {
        storageRow(
            BoardRow(
                title: "Glass Harbor",
                artist: "Ines Varga Trio",
                media: PreviewData.media(.cd),
                state: .inStorage(pinned: false),
                status: .inStorage,
                files: 9,
                megabytes: 356,
                added: "2026-08-30"
            ),
            releaseId: "rel-shot-\(number)",
            albumId: "album-shot-\(number)"
        )
    }

    private static func copperSkyRow(_ number: Int) -> BridgeStorageRow {
        storageRow(
            BoardRow(
                title: "Copper Sky",
                artist: "Odile Fen",
                media: PreviewData.media(.cd),
                state: .onOneDevice,
                status: .onOneDevice,
                files: 11,
                megabytes: 388,
                added: "2026-10-04"
            ),
            releaseId: "rel-shot-\(number)",
            albumId: "album-shot-\(number)"
        )
    }

    private static func fieldRecordingsRow(_ number: Int) -> BridgeStorageRow {
        storageRow(
            BoardRow(
                title: "Field Recordings",
                artist: "Mara Quill",
                media: PreviewData.media(.cd, 2),
                state: .onOneDevice,
                status: .uploading,
                files: 24,
                megabytes: 902,
                added: "2026-11-06"
            ),
            releaseId: "rel-shot-\(number)",
            albumId: "album-shot-\(number)",
            upload: shotUpload(queued: 0, uploading: 1)
        )
    }

    /// The outbox with the upload of `releaseId`, 62 percent of its 902 MB
    /// sent.
    @MainActor
    private static func uploadingOutbox(_ releaseId: String) -> OutboxStore {
        OutboxStore(
            uploadsPaused: false,
            releaseProgress: .releaseUploads(
                holding: [
                    releaseId: BridgeReleaseUploadTransfer(
                        bar: BridgeUploadBar(
                            bytesDone: 559_000_000,
                            bytesTotal: 902_000_000,
                            fraction: 0.62
                        ),
                        throughputBps: 6_400_000
                    )
                ]
            )
        )
    }

    // MARK: Table

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
                storageManagerEnvironment(
                    table,
                    fixture: fixture,
                    outbox: uploading.map(uploadingOutbox)
                )
            ) {
                fixture.storageManagerStore.list != nil
            }
        }
    }

    private static let fullTableWidth: CGFloat = 1_080

    /// `MacTable`: three releases, the second uploading and selected; three
    /// rows whose releases have not loaded yet; and the message over a search
    /// that finds nothing.
    private static let tableScenes: [ShotScene] = [
        storageTable(
            "mac.component.table.rows",
            size: CGSize(width: fullTableWidth, height: 150),
            rows: [
                copperSkyRow(1), fieldRecordingsRow(2), glassHarborRow(3),
            ],
            selected: ["rel-shot-2"],
            uploading: "rel-shot-2"
        ),
        storageTable(
            "mac.component.table.loading",
            size: CGSize(width: fullTableWidth, height: 150),
            rows: [copperSkyRow(1), fieldRecordingsRow(2), glassHarborRow(3)],
            unloaded: true
        ),
    ]

    // MARK: Footer

    /// `MacStorageFooter` over the board's twelve releases, 4.4 GB, with the
    /// transfer queues `queues` names and the streams their rates read:
    /// uploads at 6.4 MB/s, pins at 3.1 MB/s and exports 18 percent written.
    private static func storageFooter(
        _ id: String,
        queues: BridgeStorageQueues
    ) -> ShotScene {
        ShotScene(
            id: id,
            size: CGSize(width: 640, height: 40)
        ) {
            var rows = boardStorageRows
            rows[11].release.totalSize = 250_000_000
            let fixture = StorageManagerPreviewFixture(
                rows: rows,
                outputs: [],
                queues: queues
            )
            let footer = ShotStorageList(store: fixture.storageManagerStore) {
                store,
                list in
                MacStorageFooter(
                    list: list,
                    totalSize: store.totalSize,
                    queues: store.queues
                )
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background(Theme.surfaceElevated)
            return ShotStage(
                storageManagerEnvironment(
                    footer,
                    fixture: fixture,
                    downloads: DownloadStore(
                        releaseProgress: .downloads(
                            holding: [
                                "rel-row-1": BridgeDownloadTransferProgress(
                                    bytesDone: 140_000_000,
                                    bytesTotal: 367_000_000,
                                    fraction: 140.0 / 367.0,
                                    throughputBps: 3_100_000
                                )
                            ]
                        )
                    ),
                    outputs: OutputStore(
                        percents: .exportPercents(holding: ["rel-row-2": 18])
                    ),
                    outbox: OutboxStore(
                        uploadsPaused: false,
                        filesBeingSent: .uploadTotal(
                            holding: BridgeUploadTransfer(
                                bar: BridgeUploadBar(
                                    bytesDone: 22_400_000,
                                    bytesTotal: 57_100_000,
                                    fraction: 22.4 / 57.1
                                ),
                                throughputBps: 6_400_000
                            )
                        )
                    )
                )
            ) {
                fixture.storageManagerStore.list?.count != nil
                    && fixture.storageManagerStore.queues != nil
            }
        }
    }

    /// `MacStorageFooter`: the count and total alone, with the three queues
    /// moving, and with two held back. An export has no rate in the app: its
    /// queue counts the percent written, where the component says
    /// "18 MB/s".
    private static let storageFooterScenes: [ShotScene] = [
        storageFooter(
            "mac.component.storage-footer.releases-only",
            queues: BridgeStorageQueues(
                uploads: nil,
                downloads: nil,
                outputs: nil
            )
        ),
        storageFooter(
            "mac.component.storage-footer.queues-moving",
            queues: BridgeStorageQueues(
                uploads: BridgeUploadQueueRun(wait: nil),
                downloads: BridgeReleaseQueueRun(
                    wait: nil,
                    active: "rel-row-1"
                ),
                outputs: BridgeReleaseQueueRun(wait: nil, active: "rel-row-2")
            )
        ),
        storageFooter(
            "mac.component.storage-footer.queues-held",
            queues: BridgeStorageQueues(
                uploads: BridgeUploadQueueRun(wait: .offline),
                downloads: BridgeReleaseQueueRun(wait: .paused, active: nil),
                outputs: nil
            )
        ),
    ]

    // MARK: Toolbar and window

    /// The Storage Manager's toolbar, which only a window draws: an empty
    /// window of the toolbar's own.
    @MainActor
    private static func storageToolbar(
        filter: BridgeStorageFilter = .all,
        search: String = "",
        inspector: Bool = false,
        off: Bool = false
    ) -> ShotStage {
        ShotStage(
            Color.clear
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .modifier(
                    MacStorageToolbar(
                        filter: .constant(filter),
                        search: .constant(search),
                        inspectorPresented: .constant(inspector),
                        off: off
                    )
                )
        )
    }

    private static let storageToolbarSize = CGSize(width: 900, height: 120)

    /// `MacStorageToolbar`, in the window's title bar.
    private static let storageToolbarScenes: [ShotScene] = [
        ShotScene(
            id: "mac.component.storage-toolbar.default",
            size: storageToolbarSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageToolbar()
        },
        ShotScene(
            id: "mac.component.storage-toolbar.inspector-on",
            size: storageToolbarSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageToolbar(inspector: true)
        },
        ShotScene(
            id: "mac.component.storage-toolbar.filtered",
            size: storageToolbarSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageToolbar(filter: .remote, search: "harbor")
        },
        ShotScene(
            id: "mac.component.storage-toolbar.no-library",
            size: storageToolbarSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageToolbar(off: true)
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
            storageManagerEnvironment(window, fixture: fixture)
        ) {
            fixture.storageManagerStore.list != nil
        }
    }

    /// `MacStorageWindow`: the release list; the inspector open on a
    /// selected release; the Cloud filter and a search; and, with no library
    /// open, the window the app shows in its place.
    private static let storageWindowScenes: [ShotScene] = [
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
            id: "mac.component.storage-window.no-library",
            size: storageWindowSize,
            frame: .window(title: "Storage Manager")
        ) {
            ShotStage(StorageManagerNoLibrary())
        },
    ]
}

/// A Storage Manager list read from `store` and drawn by `content` once its
/// first rows land, as the window does.
private struct ShotStorageList<Content: View>: View {
    let store: StorageManagerStore
    @ViewBuilder
    let content: @MainActor (StorageManagerStore, StorageList) -> Content

    var body: some View {
        Group {
            if let list = store.list {
                content(store, list)
            }
        }
        .task {
            store.update(
                filter: .all,
                sort: BridgeStorageSort(
                    field: .albumTitle,
                    direction: .ascending
                ),
                search: ""
            )
        }
    }
}
