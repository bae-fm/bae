import BaeKit
import SwiftUI

/// One row of a track list: a leading column (a playlist entry's number), a
/// cover, a title over a second line, then whose it is and its length.
///
/// With `hanging`, the leading column hangs in the margin left of the row,
/// right-aligned and `leadWidth` wide, so the covers start at the list's
/// edge, as a playlist's page in the side pane draws its tracks. `wide`
/// leaves more room at the end, for a row that ends in a person. While the
/// owner drags a playlist's row, `lift` says whether the row is left faint in
/// its place or is the copy under the pointer, raised over the rows.
struct MacListRow: View {
    /// Where the row is in the owner's drag of a row.
    enum Lift {
        case resting
        /// Left faint in its place.
        case lifted
        /// The copy under the pointer.
        case dragging
    }

    let lead: String
    var leadWidth: CGFloat = 56
    var hanging = false
    var coverSize: CGFloat = MacListRow.defaultCoverSize
    var cover: BridgeImageRef?
    let title: String
    let subtitle: String
    /// The person the row names at its end, with their picture.
    var who: BridgePlaylistOwner?
    let duration: String
    var wide = false
    var hover = false
    var lift = Lift.resting

    /// The space between the row's columns.
    static let gap: CGFloat = 10
    /// How far a hanging row's leading column reaches into the margin left of
    /// its list: its width and the gap after it.
    static func hang(leadWidth: CGFloat) -> CGFloat {
        leadWidth + gap
    }

    /// The cover's size unless a row asks for another.
    static let defaultCoverSize: CGFloat = 32
    private static var durationWidth: CGFloat { 40 }

    var body: some View {
        HStack(spacing: Self.gap) {
            Text(lead)
                .themeText(.detail)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(
                    width: leadWidth,
                    alignment: hanging ? .trailing : .leading
                )
            Artwork(imageRef: cover, pointSize: coverSize)
                .frame(width: coverSize, height: coverSize)
                .clipShape(
                    // The design draws the default cover at 4, between the radius
                    // tokens, and a given size at the artwork radius.
                    RoundedRectangle(
                        cornerRadius: coverSize == Self.defaultCoverSize
                            ? 4 : ThemeRadius.artwork
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
                        picture: who.avatarVersion.map { (who.person, $0) },
                        size: 18
                    )
                    Text(verbatim: who.name)
                        .themeText(.detail)
                        .lineLimit(1)
                }
                .foregroundStyle(.secondary)
            }
            Text(duration)
                .themeText(.detail)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(width: Self.durationWidth, alignment: .trailing)
        }
        .padding(.trailing, wide ? ThemeSpace.edge : ThemeSpace.related)
        .padding(
            .leading,
            hanging ? 0 : ThemeSpace.related
        )
        .padding(.vertical, 5)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                .fill(fill)
        )
        .contentShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
        .opacity(lift == .lifted ? 0.45 : lift == .dragging ? 0.75 : 1)
        .shadow(
            color: lift == .dragging ? Theme.shadow : .clear,
            radius: ThemeSpace.group,
            y: ThemeSpace.related
        )
        .padding(.leading, hanging ? -Self.hang(leadWidth: leadWidth) : 0)
    }

    private var fill: AnyShapeStyle {
        if lift == .dragging {
            return AnyShapeStyle(Theme.surfaceElevated)
        }
        return AnyShapeStyle(hover ? Theme.hover : .clear)
    }
}
