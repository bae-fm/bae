import BaeKit
import SwiftUI

/// A user-facing error: the localized line, the concrete fault beneath it with
/// a copy action, and a disclosure for the rest of the chain.
///
/// The fault line stays outside the disclosure because core's line names only
/// a category ("Something went wrong.").
struct ErrorDetailDisclosure: View {
    let error: DisplayError?
    /// Tint for the line and icon: `Theme.danger` or `Theme.warning`.
    var tint: Color = Theme.danger
    var showIcon: Bool = true

    @State
    private var detailExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                if showIcon {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(tint)
                }
                Text(error?.line ?? "")
                    .font(.callout)
                    .foregroundStyle(tint)
            }

            if let detail = error?.detail {
                HStack(alignment: .top, spacing: 6) {
                    Text(error?.detailSummary ?? "")
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .truncationMode(.tail)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Button {
                        SystemActions.copyToPasteboard(detail)
                    } label: {
                        Label("Copy details", systemImage: "doc.on.doc")
                    }
                    .buttonStyle(.borderless)
                    .help("Copy details")
                    .accessibilityLabel("Copy details")
                }
            }

            // Only when the chain says more than the line above.
            if let detail = error?.detail, let excerpt = error?.detailExcerpt,
                detail != error?.detailSummary
            {
                Button {
                    detailExpanded = !detailExpanded
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: "chevron.right")
                            .font(.caption.weight(.semibold))
                            .rotationEffect(.degrees(detailExpanded ? 90 : 0))
                        Text("Details")
                            .font(.caption)
                        Spacer(minLength: 0)
                    }
                    .foregroundStyle(.secondary)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)

                if detailExpanded {
                    Text(excerpt)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .lineLimit(6)
                        .truncationMode(.tail)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }
}

#if DEBUG
    #Preview("Error Detail Disclosure") {
        VStack(alignment: .leading, spacing: 22) {
            // Hard failure carrying opaque detail — the disclosure row shows.
            ErrorDetailDisclosure(error: PreviewData.displayErrorWithDetail)
            // Warning tint, no detail — line only.
            ErrorDetailDisclosure(
                error: PreviewData.displayErrorSimple,
                tint: Theme.warning
            )
            // Icon suppressed (inline banner variant).
            ErrorDetailDisclosure(
                error: PreviewData.displayErrorSimple,
                showIcon: false
            )
        }
        .padding(24)
        .frame(width: 440)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
