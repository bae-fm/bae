import BaeKit
import SwiftUI

/// One master-list row in the composer/artist browser; until its summary loads
/// it shows a placeholder and cannot be selected.
struct BrowseListRow<Summary: BrowseSummaryDisplay>: View {
    @Environment(LibraryStore.self)
    private var libraryStore
    @State
    private var isHovered = false

    let id: String?
    let isSelected: Bool
    let summaries: KeyPath<LibraryStore, [String: Summary]>
    let select: (String) -> Void

    var body: some View {
        let summary = id.flatMap { libraryStore[keyPath: summaries][$0] }
        Button(action: {
            guard let id else {
                return
            }
            select(id)
        }) {
            ZStack(alignment: .leading) {
                SummaryRowPlaceholder()
                    .opacity(summary == nil ? 1 : 0)
                    .allowsHitTesting(summary == nil)
                BrowseSummaryRow(summary: summary)
                    .opacity(summary == nil ? 0 : 1)
                    .allowsHitTesting(summary != nil)
            }
            .padding(ThemeSpace.related)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .fill(rowFill)
            )
            .contentShape(RoundedRectangle(cornerRadius: ThemeRadius.control))
        }
        .buttonStyle(.plain)
        .disabled(summary == nil)
        .onHover { isHovered = $0 }
    }

    /// A selected row keeps its accent fill under hover.
    private var rowFill: Color {
        if isSelected {
            return Theme.accentSoft
        }
        return isHovered ? Theme.hover : .clear
    }
}

private struct BrowseSummaryRow<Summary: BrowseSummaryDisplay>: View {
    let summary: Summary?

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            ImageView(imageRef: summary?.image, pointSize: ThemeSize.rowArtwork)
                .frame(
                    width: ThemeSize.rowArtwork,
                    height: ThemeSize.rowArtwork
                )
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                StableOptionalText(
                    text: summary?.name,
                    font: ThemeText.rowTitle.font,
                    foreground: .primary,
                    lineHeight: 17,
                    lineLimit: 1
                )
                StableOptionalText(
                    text: summary?.countText,
                    font: ThemeText.detail.font,
                    foreground: .secondary,
                    lineHeight: 14,
                    lineLimit: 1
                )
            }
            Spacer(minLength: 0)
        }
    }
}

private struct SummaryRowPlaceholder: View {
    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                .fill(Theme.placeholder)
                .frame(
                    width: ThemeSize.rowArtwork,
                    height: ThemeSize.rowArtwork
                )
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(Theme.placeholder)
                    .frame(width: 140, height: 11)
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(Theme.placeholder)
                    .frame(width: 80, height: 10)
            }
            Spacer(minLength: 0)
        }
    }
}

#if DEBUG
    #Preview("Browse List Row") {
        let libraryStore = PreviewData.seededComposerStore()
        return VStack(spacing: ThemeSpace.inline) {
            BrowseListRow(
                id: "composer-0",
                isSelected: false,
                summaries: \.composerSummaries,
                select: { _ in }
            )
            BrowseListRow(
                id: "composer-1",
                isSelected: true,
                summaries: \.composerSummaries,
                select: { _ in }
            )
            // Not in the store: renders the placeholder.
            BrowseListRow(
                id: "composer-unloaded",
                isSelected: false,
                summaries: \.composerSummaries,
                select: { _ in }
            )
        }
        .padding()
        .frame(width: 320)
        .environment(libraryStore)
        .environment(ImageStore.stub())
    }
#endif
