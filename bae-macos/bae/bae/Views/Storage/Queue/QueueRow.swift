import BaeKit
import Foundation
import SwiftUI

/// The layout shared by the download and export queue rows: the content with
/// its enqueue time and optional cancel button, then what sits under the row.
struct QueueRow<Content: View, Below: View>: View {
    /// The cancel button's action and the tooltip naming what it abandons.
    struct CancelAction {
        let help: LocalizedStringKey
        let action: () -> Void
    }

    let icon: String
    /// When the entry was queued, in milliseconds since the epoch; nil for a
    /// row that shows no time.
    let createdAt: Int64?
    var cancel: CancelAction?
    @ViewBuilder
    let content: () -> Content
    /// What sits under the row at its edge, such as a failure's notice, whose
    /// glyph takes the row's leading column.
    @ViewBuilder
    let below: () -> Below

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            RowItem {
                Image(systemName: icon)
                    .themeIcon(.medium)
                    .foregroundStyle(.secondary)
            } content: {
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    content()
                }
            } trail: {
                if let createdAt {
                    Text(queuedRelativeLabel(createdAt))
                        .themeText(.fine)
                        .foregroundStyle(.secondary)
                        .fixedSize()
                }

                if let cancel {
                    Button(action: cancel.action) {
                        Image(systemName: "xmark.circle")
                    }
                    .buttonStyle(.plain)
                    .help(cancel.help)
                }
            }

            below()
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
            Text(verbatim: "Album Title")
                .lineLimit(1)
            StatusChip("Queued to pin", symbol: "clock")
        } below: {
            EmptyView()
        }
        .frame(width: 640)
        .padding(.vertical)
    }
#endif
