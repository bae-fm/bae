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

    /// When an entry was queued, and the moment its list words that against,
    /// which the list reads once for all its rows.
    struct Queued {
        /// The enqueue time, in milliseconds since the epoch.
        let createdAt: Int64
        let now: Date
    }

    let icon: String
    /// When the entry was queued, worded against the moment the whole list
    /// shares; nil for a row that shows no time.
    var queued: Queued?
    var cancel: CancelAction?
    @ViewBuilder
    let content: () -> Content
    /// What sits under the row at its edge, such as a failure's notice, whose
    /// glyph takes the row's leading column.
    @ViewBuilder
    let below: () -> Below

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            MacItem {
                Image(systemName: icon)
                    .font(.system(size: ThemeSize.lead))
                    .foregroundStyle(Theme.macTextSecondary)
                    .accessibilityHidden(true)
            } content: {
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    content()
                }
            } trail: {
                if let queued {
                    Text(
                        Date(
                            timeIntervalSince1970: TimeInterval(
                                queued.createdAt
                            )
                                / 1000
                        )
                        .agoLine(now: queued.now, style: .abbreviated)
                    )
                    .themeText(.fine)
                    .fixedSize()
                }

                if let cancel {
                    Button(action: cancel.action) {
                        Image(systemName: "xmark.circle")
                    }
                    .buttonStyle(.accentPlain)
                    .help(cancel.help)
                    .accessibilityLabel(Text(cancel.help))
                }
            }

            below()
        }
        .padding(.horizontal)
        .padding(.vertical, ThemeSpace.related)
    }
}
