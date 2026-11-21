import AppKit
import BaeKit
import SwiftUI

@testable import bae

// Fixtures that more than one component's capture file draws with.

extension ShotScene {
    /// Glass Harbor, in the cloud unless `state` and `status` say otherwise.
    static func glassHarborRow(
        _ number: Int,
        state: BridgeReleaseStorageState = .inStorage(pinned: false),
        status: BridgeStorageRowStatus = .inStorage
    ) -> BridgeStorageRow {
        storageRow(
            BoardRow(
                title: "Glass Harbor",
                artist: "Ines Varga Trio",
                media: PreviewData.media(.cd),
                state: state,
                status: status,
                files: 9,
                megabytes: 356,
                added: "2026-08-30"
            ),
            releaseId: "rel-shot-\(number)",
            albumId: "album-shot-\(number)"
        )
    }

    static func fieldRecordingsRow(_ number: Int) -> BridgeStorageRow {
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
    static func uploadingOutbox(_ releaseId: String) -> OutboxStore {
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
}

/// A Storage Manager list read from `store` and drawn by `content` once its
/// first rows land, as the window does.
struct ShotStorageList<Content: View>: View {
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
