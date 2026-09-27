import BaeKit
import SwiftUI

/// The image, name and count header atop a composer's or artist's detail pane.
struct BrowseDetailHeader<Summary: BrowseSummaryDisplay>: View {
    let summary: Summary

    private static var imageSize: CGFloat { 72 }

    var body: some View {
        HStack(spacing: ThemeSpace.edge) {
            ImageView(imageRef: summary.image, pointSize: Self.imageSize)
                .frame(width: Self.imageSize, height: Self.imageSize)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(summary.name)
                    .themeText(.title)
                    .lineLimit(2)
                Text(summary.countText)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

#if DEBUG
    #Preview("Browse Detail Header") {
        VStack(alignment: .leading, spacing: ThemeSpace.section) {
            BrowseDetailHeader(summary: PreviewData.composerSummary)
            BrowseDetailHeader(summary: PreviewData.artistSummary)
        }
        .padding()
        .frame(width: 420, alignment: .leading)
        .environment(ImageStore.stub())
    }
#endif
