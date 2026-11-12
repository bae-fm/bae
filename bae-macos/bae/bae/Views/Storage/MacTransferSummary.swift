import BaeKit
import SwiftUI

/// A release's upload or pin in the Storage Manager's inspector: how many of
/// its files are moving, waiting and done, as core counts them, with the
/// button that cancels the transfer; while files are moving, the bytes of its
/// size stored or sent and how fast they go, over a bar of the same bytes,
/// which a native readout sets from the transfer's stream.
struct MacTransferSummary: View {
    let kind: StorageTransferKind
    let counts: [BridgeTransferCount]
    /// Whose transfer the readout follows.
    let releaseId: String
    /// Whether files are moving now, rather than waiting their turn: the
    /// readout shows only then.
    let moving: Bool
    let onCancel: () -> Void

    /// The gap between the summary's lines, and its inset above and below.
    private static let lineGap: CGFloat = 10
    private static let verticalInset: CGFloat = 14

    var body: some View {
        VStack(alignment: .leading, spacing: Self.lineGap) {
            HStack(spacing: ThemeSpace.related) {
                StorageTransferCounts(counts: counts, kind: kind)
                Spacer()
                Button("Cancel", action: onCancel)
                    .controlSize(.small)
            }
            if moving {
                TransferReadoutContainer(kind: kind, releaseId: releaseId)
            }
        }
        .themeText(.detail)
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, Self.verticalInset)
    }
}

/// Reads the release's transfer stream for `MacTransferSummary`'s readout, so
/// only the readout follows the bytes as they move.
private struct TransferReadoutContainer: View {
    let kind: StorageTransferKind
    let releaseId: String
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        switch kind {
        case .upload:
            TransferReadout(
                stream: .upload(
                    outboxStore.uploadProgress(forRelease: releaseId)
                ),
                kind: .upload
            )
        case .pin:
            TransferReadout(
                stream: .pin(
                    downloadStore.downloadProgress(forRelease: releaseId)
                ),
                kind: .pin
            )
        }
    }
}
