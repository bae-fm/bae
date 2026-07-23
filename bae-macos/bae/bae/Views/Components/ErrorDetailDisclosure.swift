import BaeKit
import SwiftUI

/// Renders a user-facing error: the generic localized line, plus — when a
/// diagnostic carries opaque detail — a collapsible disclosure exposing the
/// untranslated Rust error chain with a copy affordance. The single surface
/// every event-driven error reuses (sync banner, inline import error); the
/// global alert renders the same `DisplayError` through alert actions instead.
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

            if let detail = error.detail {
                DisclosureGroup(isExpanded: $detailExpanded) {
                    HStack(alignment: .top, spacing: 6) {
                        Text(detail)
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
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
                } label: {
                    Text("Details")
                        .font(.caption)
                        .foregroundStyle(.secondary)
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
