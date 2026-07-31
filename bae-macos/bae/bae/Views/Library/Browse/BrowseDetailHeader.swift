import BaeKit
import SwiftUI

/// The 72 pt image + name + count header atop a composer's or artist's detail
/// pane.
struct BrowseDetailHeader<Summary: BrowseSummaryDisplay>: View {
    let summary: Summary

    var body: some View {
        HStack(spacing: 16) {
            ImageView(imageRef: summary.image, pointSize: 72)
                .frame(width: 72, height: 72)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            VStack(alignment: .leading, spacing: 5) {
                Text(summary.name)
                    .font(.system(size: 22, weight: .bold))
                    .tracking(-0.3)
                    .lineLimit(2)
                Text(summary.countText)
                    .font(.system(size: 13))
                    .foregroundStyle(.secondary)
            }
        }
    }
}

#if DEBUG
    #Preview("Browse Detail Header") {
        VStack(alignment: .leading, spacing: 24) {
            BrowseDetailHeader(summary: PreviewData.composerSummary)
            BrowseDetailHeader(summary: PreviewData.artistSummary)
        }
        .padding()
        .frame(width: 420, alignment: .leading)
        .environment(ImageStore.stub)
    }
#endif
