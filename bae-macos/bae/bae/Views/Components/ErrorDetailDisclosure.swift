import BaeKit
import SwiftUI

/// Renders a user-facing error: the generic localized line, the concrete fault
/// beneath it, and — when there is more chain than fits — a collapsible
/// disclosure exposing the rest with a copy affordance. The single surface every
/// event-driven error reuses (sync banner, inline import error); the global
/// alert renders the same `DisplayError` through alert actions instead.
///
/// The fault line is not behind the disclosure. Core's line names a category
/// ("Something went wrong."), so a reader who does not open the disclosure is
/// told nothing about what failed — which is how a failing sync cycle spent an
/// hour looking like a generic internal error.
struct ErrorDetailDisclosure: View {
    let error: DisplayError
    /// Tint for the line and icon — red for hard failures, orange for warnings.
    var tint: Color = .red
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
                Text(error.line)
                    .font(.callout)
                    .foregroundStyle(tint)
            }

            if let summary = error.detailSummary {
                Text(summary)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .truncationMode(.tail)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            // Only when the chain says more than the line above already did —
            // a disclosure that expands to the same sentence is a control that
            // does nothing.
            if let detail = error.detail, let excerpt = error.detailExcerpt,
                detail != error.detailSummary
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
                    HStack(alignment: .top, spacing: 6) {
                        Text(excerpt)
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
                            .lineLimit(6)
                            .truncationMode(.tail)
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Button {
                            SystemActions.copyToPasteboard(detail)
                        } label: {
                            Image(systemName: "doc.on.doc")
                        }
                        .buttonStyle(.borderless)
                        .help("Copy details")
                    }
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
                tint: .orange
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
