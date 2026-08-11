import BaeKit
import SwiftUI

/// One master-list row in the composer/artist browser. The summary may not be
/// loaded yet, so the row keeps both a shimmer placeholder and the real content
/// in the layout tree, opacity-swapped, and disables selection until the summary
/// arrives.
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
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 7)
                    .fill(rowFill)
            )
            .contentShape(RoundedRectangle(cornerRadius: 7))
        }
        .buttonStyle(.plain)
        .disabled(summary == nil)
        .onHover { isHovered = $0 }
    }

    /// Selected rows carry the soft accent fill and keep it under hover; an
    /// unselected row lifts to a faint foreground wash while hovered.
    private var rowFill: Color {
        if isSelected {
            return Theme.accentSoft
        }
        return isHovered ? Color.primary.opacity(0.04) : .clear
    }
}

private struct BrowseSummaryRow<Summary: BrowseSummaryDisplay>: View {
    let summary: Summary?

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary?.image, pointSize: 40)
                .frame(width: 40, height: 40)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 2) {
                StableOptionalText(
                    text: summary?.name,
                    font: .browseRowTitle,
                    foreground: .primary,
                    lineHeight: 17,
                    lineLimit: 1
                )
                StableOptionalText(
                    text: summary?.countText,
                    font: .browseRowCaption,
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
        HStack(spacing: 12) {
            RoundedRectangle(cornerRadius: 6)
                .fill(.secondary.opacity(0.15))
                .frame(width: 40, height: 40)
            VStack(alignment: .leading, spacing: 5) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.15))
                    .frame(width: 140, height: 11)
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.12))
                    .frame(width: 80, height: 10)
            }
            Spacer(minLength: 0)
        }
    }
}

#if DEBUG
    #Preview("Browse List Row") {
        let libraryStore = PreviewData.seededComposerStore()
        return VStack(spacing: 4) {
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
            // Not in the store: renders the shimmer placeholder.
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
