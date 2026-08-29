import Foundation
import SwiftUI

/// Row scaffolding shared by the download and export queue
/// rows: a title/detail block with enqueue time and cancel action, then the
/// state on its own line. The vertical shape leaves the title readable in the
/// transfer inspector instead of forcing every field into one horizontal row.
///
/// The cancel button is optional — a row for work that cannot be abandoned (a
/// cloud tombstone, whose object would otherwise be stranded) passes no
/// `cancel` and keeps the trailing space empty.
struct QueueRow<Content: View, Badge: View>: View {
    /// The cancel affordance as one value: a button always ships with the
    /// tooltip naming what it abandons.
    struct CancelAction {
        let help: LocalizedStringKey
        let action: () -> Void
    }

    let icon: String
    let createdAt: Int64
    var cancel: CancelAction?
    @ViewBuilder
    let content: () -> Content
    @ViewBuilder
    let badge: () -> Badge

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: icon)
                    .foregroundStyle(.secondary)
                    .frame(width: 16)

                VStack(alignment: .leading, spacing: 3) {
                    content()
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                Text(queuedRelativeLabel(createdAt))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize()

                if let cancel {
                    Button(action: cancel.action) {
                        Image(systemName: "xmark.circle")
                    }
                    .buttonStyle(.plain)
                    .help(cancel.help)
                }
            }

            badge()
                .font(.caption)
                .lineLimit(1)
                .padding(.leading, 28)
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
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
        // Routed through a String so the extractor never takes preview-only
        // copy into the catalog.
        let sampleHelp = "Cancel this item"
        QueueRow(
            icon: "arrow.down.circle",
            createdAt: Int64(Date().timeIntervalSince1970 * 1000) - 120_000,
            cancel: .init(
                help: LocalizedStringKey(sampleHelp),
                action: {}
            )
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
