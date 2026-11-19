import BaeKit
import SwiftUI

/// One row of a track list: a leading column (a playlist entry's number, a
/// play's time), a cover, a title over a second line, then whose it is and
/// its length when given.
///
/// `hanging` hangs the leading column in the margin left of the row,
/// right-aligned and `leadWidth` wide, so the covers start at the list's
/// edge, and puts the second line 3pt under the title, as a playlist's page
/// and History in the side pane draw their rows. `wide` leaves more room at
/// the end, for a row that ends in a person. While the owner drags a
/// playlist's row, `lifted` is the row left faint in its place and `dragging`
/// the copy under the pointer, raised over the rows.
struct MacListRow: View {
    /// A person a row names at its end, with their picture.
    struct Who {
        let name: String
        let initials: String
        var picture: (person: String, version: String)?
    }

    let lead: String
    var hanging = false
    var leadWidth: CGFloat = 20
    var cover: BridgeImageRef?
    var coverSize: CGFloat = 32
    let title: String
    let subtitle: String
    var who: Who?
    var duration: String?
    var wide = false
    var hover = false
    var lifted = false
    var dragging = false

    /// The space between the row's columns.
    static let gap: CGFloat = 10
    /// How far a hanging row's leading column reaches into the margin left
    /// of its list: its width and the gap after it.
    static func hang(leadWidth: CGFloat) -> CGFloat {
        leadWidth + gap
    }

    private static var restingLeadWidth: CGFloat { 56 }
    private static var durationWidth: CGFloat { 40 }

    var body: some View {
        HStack(spacing: Self.gap) {
            Text(lead)
                .themeText(.detail)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(
                    width: hanging ? leadWidth : Self.restingLeadWidth,
                    alignment: hanging ? .trailing : .leading
                )
            ImageView(imageRef: cover, pointSize: coverSize)
                .frame(width: coverSize, height: coverSize)
                .clipShape(
                    RoundedRectangle(
                        cornerRadius: hanging
                            ? ThemeRadius.artwork : ThemeRadius.bar
                    )
                )
            VStack(alignment: .leading, spacing: hanging ? 3 : 0) {
                Text(title)
                    .themeText(.rowTitle)
                    .lineLimit(1)
                Text(subtitle)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if let who {
                HStack(spacing: ThemeSpace.compact) {
                    Avatar(
                        initials: who.initials,
                        picture: who.picture,
                        size: 18
                    )
                    Text(verbatim: who.name)
                        .themeText(.detail)
                        .lineLimit(1)
                }
                .foregroundStyle(.secondary)
            }
            if let duration {
                Text(duration)
                    .themeText(.detail)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(width: Self.durationWidth, alignment: .trailing)
            }
        }
        .padding(.leading, hanging ? 0 : ThemeSpace.related)
        .padding(.trailing, wide ? ThemeSpace.edge : ThemeSpace.related)
        .padding(.vertical, 5)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                .fill(fill)
        )
        .contentShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
        .opacity(lifted ? 0.45 : dragging ? 0.75 : 1)
        .shadow(
            color: dragging ? Theme.shadow : .clear,
            radius: ThemeSpace.group,
            y: ThemeSpace.related
        )
        .padding(.leading, hanging ? -Self.hang(leadWidth: leadWidth) : 0)
    }

    private var fill: AnyShapeStyle {
        if dragging {
            return AnyShapeStyle(Theme.surfaceElevated)
        }
        return AnyShapeStyle(hover ? Theme.hover : .clear)
    }
}
