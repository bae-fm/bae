import BaeKit
import Foundation
import SwiftUI

/// Footer under the storage table: the current filter's release count and
/// total size on the left; on the right, each transfer queue holding work,
/// with how fast it goes and the control that pauses or resumes it whole.
struct MacStorageFooter: View {
    let list: StorageList
    /// The size of every row the current filter matches; nil, and not shown,
    /// until fetched.
    let totalSize: UInt64?
    /// The transfer queues holding work; nil, and none shown, until core
    /// answers.
    let queues: BridgeStorageQueues?

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            if let count = list.count {
                // An Int, so the text reads the catalog's "%lld releases".
                Text("\(Int(count.rows)) releases")
            }
            if let totalSize {
                Text(verbatim: FactList.glyph)
                Text(
                    "Total: \(totalSize.fileSizeText)"
                )
            }
            Spacer()
            if let uploads = queues?.uploads {
                MacQueueRate(
                    queue: .uploads,
                    paused: uploads.wait == .paused(by: .person),
                    hold: uploads.wait?.text
                ) {
                    StorageUploadRate()
                }
            }
            if let downloads = queues?.downloads {
                MacQueueRate(
                    queue: .downloads,
                    paused: downloads.wait == .paused,
                    hold: downloads.wait?.text
                ) {
                    if let active = downloads.active {
                        StoragePinRate(releaseId: active)
                    }
                }
            }
            if let outputs = queues?.outputs {
                MacQueueRate(
                    queue: .outputs,
                    paused: outputs.wait == .paused,
                    hold: outputs.wait?.text
                ) {
                    if let active = outputs.active {
                        StorageOutputPercent(releaseId: active)
                    }
                }
            }
        }
        .lineLimit(1)
        .foregroundStyle(Theme.macTextSecondary)
        .themeText(.body)
        .padding(.horizontal)
        .padding(.vertical, ThemeSpace.related)
    }
}

/// How fast every file being sent goes together, which a native label reads
/// from the latest total each frame. A zero reading shows nothing.
private struct StorageUploadRate: View {
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        StreamLabel(stream: .uploadRate(outboxStore.totalProgress))
    }
}

/// How fast the pin queue's running release has downloaded, which a native
/// label reads from the release's latest progress each frame.
private struct StoragePinRate: View {
    let releaseId: String
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        StreamLabel(
            stream: .pinRate(
                downloadStore.downloadProgress(forRelease: releaseId)
            )
        )
    }
}

/// How far the output queue's running release has been written, which a
/// native label reads from the release's latest percent each frame.
private struct StorageOutputPercent: View {
    let releaseId: String
    @Environment(OutputStore.self)
    private var outputStore

    var body: some View {
        StreamLabel(
            stream: .exportPercent(
                outputStore.exportProgress(forRelease: releaseId)
            )
        )
    }
}
