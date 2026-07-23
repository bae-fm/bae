import BaeKit
import SwiftUI

/// The storage badge: an in-flight transfer or queued upload wins over the
/// resting storage state.
struct StorageStateLabel: View {
    let release: ReleaseSummary
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        if let transfer = release.transfer {
            Label(transfer.label, systemImage: "arrow.down.circle")
                .foregroundStyle(.blue)
                .lineLimit(1)
        }
        else if let progress = outboxStore.progress(forRelease: release.id),
            let activity = progress.activity
        {
            uploadBadge(activity, remaining: Int(progress.pending))
        }
        else {
            switch release.storageState {
            case .local:
                Label("Unmanaged", systemImage: "folder").lineLimit(1)
            case .remote:
                if release.pinned {
                    Label("Pinned", systemImage: "pin.fill").lineLimit(1)
                }
                else {
                    Label("Cloud", systemImage: "cloud").lineLimit(1)
                }
            }
        }
    }

    /// The per-release upload badge: only a release with a file in flight reads
    /// as "Uploading"; one with work still waiting reads as "Queued", and one
    /// stalled on failures awaiting retry as "Retrying". A release with
    /// nothing left to ship has no badge — it falls back to the resting
    /// state. `remaining` is the release's unshipped file count.
    @ViewBuilder
    private func uploadBadge(
        _ activity: BridgeUploadActivity,
        remaining: Int
    ) -> some View {
        switch activity {
        case .uploading:
            Label("Uploading (\(remaining))", systemImage: "arrow.up.circle")
                .foregroundStyle(.orange)
                .lineLimit(1)
        case .queued:
            Label("Queued (\(remaining))", systemImage: "clock")
                .foregroundStyle(.secondary)
                .lineLimit(1)
        case .retrying:
            Label(
                "Retrying (\(remaining))",
                systemImage: "exclamationmark.triangle.fill"
            )
            .foregroundStyle(.red)
            .lineLimit(1)
        }
    }
}
