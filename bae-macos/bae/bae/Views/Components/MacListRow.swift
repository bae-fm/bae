import BaeKit
import SwiftUI

/// One row of a track list on a page: a leading number, a cover, a title over
/// a second line, and a trailing duration.
struct MacListRow: View {
    let lead: String
    var cover: BridgeImageRef?
    let title: String
    let subtitle: String
    let duration: String
    var hover = false

    private static var leadWidth: CGFloat { 56 }
    private static var durationWidth: CGFloat { 40 }
    private static var coverSize: CGFloat { 32 }

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Text(lead)
                .themeText(.detail)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(width: Self.leadWidth, alignment: .center)
            ImageView(imageRef: cover, pointSize: Self.coverSize)
                .frame(width: Self.coverSize, height: Self.coverSize)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.bar))
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(title)
                    .themeText(.rowTitle)
                    .lineLimit(1)
                Text(subtitle)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: ThemeSpace.related)
            Text(duration)
                .themeText(.detail)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(width: Self.durationWidth, alignment: .trailing)
        }
        .padding(.horizontal, ThemeSpace.related)
        .padding(.vertical, ThemeSpace.inline)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.control)
                .fill(hover ? Theme.hover : .clear)
        )
        .contentShape(RoundedRectangle(cornerRadius: ThemeRadius.control))
    }
}

#if DEBUG
    #Preview("Mac List Row") {
        VStack(spacing: 0) {
            MacListRow(
                lead: "1",
                title: "Harbor Lights",
                subtitle: "Mara Quill · Low Tide Hours",
                duration: "4:32"
            )
            MacListRow(
                lead: "2",
                title: "Copper Sky",
                subtitle: "Odile Fen · Copper Sky",
                duration: "3:58",
                hover: true
            )
        }
        .padding()
        .frame(width: 520)
        .environment(ImageStore.stub())
    }
#endif
