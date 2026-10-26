import BaeKit
import SwiftUI

/// One master-list row in the composer/artist browser, drawn from the row the
/// list's query returned at its position; until that row's page loads it
/// shows a placeholder and cannot be selected.
struct BrowseListRow<
    Summary: BrowseSummaryDisplay & Identifiable
>: View where Summary.ID == String {
    @State
    private var isHovered = false

    let summary: Summary?
    let isSelected: Bool
    let select: (String) -> Void

    var body: some View {
        Button(action: {
            guard let summary else {
                return
            }
            select(summary.id)
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
        VStack(spacing: ThemeSpace.inline) {
            BrowseListRow(
                summary: PreviewData.composerSummaries[0],
                isSelected: false,
                select: { _ in }
            )
            BrowseListRow(
                summary: PreviewData.composerSummaries[1],
                isSelected: true,
                select: { _ in }
            )
            // Not loaded: renders the placeholder.
            BrowseListRow<BridgeComposerSummary>(
                summary: nil,
                isSelected: false,
                select: { _ in }
            )
        }
        .padding()
        .frame(width: 320)
        .environment(ImageStore.stub())
    }
#endif
