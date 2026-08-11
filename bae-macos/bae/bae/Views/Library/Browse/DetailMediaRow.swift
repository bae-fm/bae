import BaeKit
import SwiftUI

/// A tappable image + title + secondary-line row shared by the works list and
/// the selected work's releases. The image slot shows the placeholder when the
/// entity has no art.
struct DetailMediaRow: View {
    let image: BridgeImageRef?
    let title: String
    let subtitle: String?

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: image, pointSize: 42)
                .frame(width: 42, height: 42)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.browseRowTitle)
                    .lineLimit(1)
                StableOptionalText(
                    text: subtitle,
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

#if DEBUG
    #Preview("Detail Media Row") {
        VStack(alignment: .leading, spacing: 8) {
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
