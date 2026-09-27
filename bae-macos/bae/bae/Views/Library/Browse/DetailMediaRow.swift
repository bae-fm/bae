import BaeKit
import SwiftUI

/// An image, title and subtitle row shared by the works list and the selected
/// work's releases.
struct DetailMediaRow: View {
    let image: BridgeImageRef?
    let title: String
    let subtitle: String?

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            ImageView(imageRef: image, pointSize: ThemeSize.rowArtwork)
                .frame(
                    width: ThemeSize.rowArtwork,
                    height: ThemeSize.rowArtwork
                )
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(title)
                    .themeText(.rowTitle)
                    .lineLimit(1)
                StableOptionalText(
                    text: subtitle,
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

#if DEBUG
    #Preview("Detail Media Row") {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            DetailMediaRow(
                image: nil,
                title: "Work Title",
                subtitle: "Composer Name"
            )
            DetailMediaRow(
                image: nil,
                title: "Album Title",
                subtitle: nil
            )
        }
        .padding()
        .frame(width: 420, alignment: .leading)
        .environment(ImageStore.stub())
    }
#endif
