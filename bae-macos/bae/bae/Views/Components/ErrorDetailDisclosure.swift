import BaeKit
import SwiftUI

/// A user-facing error: the localized line, the concrete fault with a copy
/// action, and a disclosure for the rest of the chain.
struct ErrorDetailDisclosure: View {
    let error: DisplayError?
    /// Tint for the line and icon: `Theme.danger` or `Theme.warning`.
    var tint: Color = Theme.danger
    var showIcon: Bool = true

    @State
    private var detailExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            HStack(spacing: ThemeSpace.related) {
                if showIcon {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(tint)
                }
                Text(error?.line ?? "")
                    .themeText(.body)
                    .foregroundStyle(tint)
            }

            if let detail = error?.detail {
                HStack(alignment: .top, spacing: ThemeSpace.compact) {
                    Text(error?.detailSummary ?? "")
                        .themeText(.mono)
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
                    HStack(spacing: ThemeSpace.inline) {
                        Image(systemName: "chevron.right")
                            .themeIcon(.small)
                            .rotationEffect(.degrees(detailExpanded ? 90 : 0))
                        Text("Details")
                            .themeText(.detail)
                        Spacer(minLength: 0)
                    }
                    .foregroundStyle(.secondary)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)

                if detailExpanded {
                    Text(excerpt)
                        .themeText(.mono)
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
        VStack(alignment: .leading, spacing: ThemeSpace.section) {
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
        .padding(ThemeSpace.section)
        .frame(width: 440)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
