import BaeKit
import Foundation
import SwiftUI

/// The layout shared by the download and export queue rows: the content with
/// its enqueue time and optional cancel button, then the state on its own line.
struct QueueRow<Content: View, Badge: View>: View {
    /// The cancel button's action and the tooltip naming what it abandons.
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
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            HStack(alignment: .top, spacing: ThemeSpace.group) {
                Image(systemName: icon)
                    .themeIcon(.medium)
                    .foregroundStyle(.secondary)
                    .frame(width: ThemeIcon.medium.size)

                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    content()
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                Text(queuedRelativeLabel(createdAt))
                    .themeText(.fine)
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
                .themeText(.detail)
                .lineLimit(1)
                // Under the content, past the icon column.
                .padding(.leading, ThemeIcon.medium.size + ThemeSpace.group)
        }
        .padding(.horizontal)
        .padding(.vertical, ThemeSpace.related)
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
