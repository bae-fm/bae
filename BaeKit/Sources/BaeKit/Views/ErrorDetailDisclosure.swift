import SwiftUI

#if canImport(AppKit)
    import AppKit
#else
    import UIKit
#endif

/// A user-facing error: the localized line, the concrete fault with a copy
/// action, and a disclosure for the rest of the chain.
public struct ErrorDetailDisclosure: View {
    private let error: DisplayError?
    /// The line's and glyph's colour; the glyph is the tone's.
    private let tone: StatusTone
    private let showIcon: Bool

    public init(
        error: DisplayError?,
        tone: StatusTone = .danger,
        showIcon: Bool = true
    ) {
        self.error = error
        self.tone = tone
        self.showIcon = showIcon
    }

    @State
    private var detailExpanded = false

    public var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            HStack(spacing: ThemeSpace.related) {
                if showIcon, let symbol = tone.symbol {
                    Image(systemName: symbol)
                        .foregroundStyle(tone.color)
                }
                Text(error?.line ?? "")
                    .themeText(.body)
                    .foregroundStyle(tone.color)
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
                        copyToPasteboard(detail)
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

private func copyToPasteboard(_ value: String) {
    #if canImport(AppKit)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(value, forType: .string)
    #else
        UIPasteboard.general.string = value
    #endif
}
