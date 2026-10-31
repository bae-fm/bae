import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Storage Manager's scenes: its list in each state, its inspector
    /// on uploads, pins and a file's details, and its whole window.
    static let storageManagerScenes: [ShotScene] = [
        ShotScene(
            id: "storage-manager-dense",
            size: CGSize(width: 1_440, height: 900)
        ) {
            AnyView(
                StorageManagerPreviewScene(selectedReleaseId: "rel-row-1")
            )
        },
        ShotScene(
            id: "storage-manager-empty",
            size: CGSize(width: 940, height: 600)
        ) {
            AnyView(StorageManagerPreviewScene(rows: []))
        },
        ShotScene(
            id: "storage-manager-empty-ish",
            size: CGSize(width: 940, height: 600)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    rows: Array(PreviewData.storageRows.prefix(2))
                )
            )
        },
        ShotScene(
            id: "storage-manager-empty-ish-inspector",
            size: CGSize(width: 940, height: 600)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    rows: Array(PreviewData.storageRows.prefix(2)),
                    inspectorPresented: true
                )
            )
        },
        ShotScene(
            id: "storage-manager-active-file-inspector",
            size: CGSize(width: 940, height: 700)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    selectedReleaseId: "rel-row-1",
                    inspectorPresented: true,
                    downloads: [],
                    outputs: []
                )
            )
        },
        ShotScene(
            id: "storage-manager-idle-file-inspector",
            size: CGSize(width: 940, height: 700)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    selectedReleaseId: "rel-row-1",
                    inspectorPresented: true,
                    downloads: [],
                    outputs: [],
                    uploads: []
                )
            )
        },
        ShotScene(
            id: "storage-manager-one-sync-inspector",
            size: CGSize(width: 940, height: 600)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    rows: Array(PreviewData.storageRows.prefix(2)),
                    selectedReleaseId: "rel-row-2",
                    inspectorPresented: true,
                    downloads: [],
                    outputs: [],
                    uploads: [PreviewData.queuedRelease]
                )
            )
        },
        // A release pinning, nothing else running: the pinning panel with
        // its audio downloading in a filling ring, and the footer's one pin
        // group.
        ShotScene(
            id: "storage-manager-pinning-inspector",
            size: CGSize(width: 1_040, height: 654)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    selectedReleaseId: "rel-row-5",
                    inspectorPresented: true,
                    downloads: PreviewData.pinningDownloads,
                    outputs: [],
                    uploads: []
                )
            )
        },
        // A release whose pin waits its turn in the download queue: its
        // files waiting and kept, and no bytes yet.
        ShotScene(
            id: "storage-manager-pin-queued-inspector",
            size: CGSize(width: 1_040, height: 654)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    selectedReleaseId: "rel-row-5",
                    inspectorPresented: true,
                    downloads: PreviewData.queuedPinDownloads,
                    outputs: [],
                    uploads: []
                )
            )
        },
        // Releases uploading while one pins: both chips filling, and the
        // footer's upload and pin groups.
        ShotScene(
            id: "storage-manager-uploads-and-pinning",
            size: CGSize(width: 1_440, height: 900)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    selectedReleaseId: "rel-row-5",
                    inspectorPresented: true,
                    downloads: PreviewData.pinningDownloads,
                    outputs: []
                )
            )
        },
        // A file of a release uploading selected: its details under the list.
        ShotScene(
            id: "storage-manager-file-details",
            size: CGSize(width: 1_040, height: 654)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    selectedReleaseId: "rel-row-1",
                    inspectorPresented: true,
                    selectedFileId: "rel-row-1-audio",
                    downloads: [],
                    outputs: []
                )
            )
        },
        // A search that finds nothing, in the whole window: the toolbar's
        // filter field holding the text, and the list saying so.
        ShotScene(
            id: "storage-manager-no-matches",
            size: CGSize(width: 1_040, height: 654),
            frame: .window(title: String(localized: "Storage Manager"))
        ) {
            AnyView(
                StorageManagerPreviewScene(rows: [], search: "Album Title")
            )
        },
        // The whole window, its toolbar's All / Local / Cloud filter, filter
        // field and inspector toggle included.
        ShotScene(
            id: "storage-manager-window",
            size: CGSize(width: 1_040, height: 654),
            frame: .window(title: String(localized: "Storage Manager"))
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    selectedReleaseId: "rel-row-1",
                    inspectorPresented: true,
                    downloads: [],
                    outputs: []
                )
            )
        },
    ]
}
