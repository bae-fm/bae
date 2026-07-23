import Foundation
import SwiftUI

/// Row scaffolding shared by the download, export, and outbox-delete queue
/// rows: leading icon, caller content, then the trailing state badge /
/// queued-time / cancel cluster.
struct QueueRow<Content: View, Badge: View>: View {
    let icon: String
    let createdAt: Int64
    let cancelHelp: LocalizedStringKey
    let onCancel: () -> Void
    @ViewBuilder
    let content: () -> Content
    @ViewBuilder
    let badge: () -> Badge

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: icon)
                .foregroundStyle(.secondary)
                .frame(width: 16)

            content()

            Spacer()

            badge()
                .font(.caption)
                .frame(width: 130, alignment: .leading)

            Text(queuedRelativeLabel(createdAt))
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 90, alignment: .trailing)

            Button(action: onCancel) {
                Image(systemName: "xmark.circle")
            }
            .buttonStyle(.plain)
            .help(cancelHelp)
        }
        .padding(.horizontal)
        .padding(.vertical, 6)
    }
}

/// Relative "queued" label (e.g. "2m ago") from an enqueue time in Unix epoch
/// milliseconds.
func queuedRelativeLabel(_ epochMs: Int64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(epochMs) / 1000)
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .abbreviated
    return formatter.localizedString(for: date, relativeTo: Date())
}

#if DEBUG
    #Preview("Queue row") {
        QueueRow(
            icon: "arrow.down.circle",
            createdAt: Int64(Date().timeIntervalSince1970 * 1000) - 120_000,
            cancelHelp: "Cancel this item",
            onCancel: {}
        ) {
            Text("Album Title")
                .lineLimit(1)
        } badge: {
            Label("Queued", systemImage: "clock")
                .foregroundStyle(.secondary)
        }
        .frame(width: 640)
        .padding(.vertical)
    }
#endif
