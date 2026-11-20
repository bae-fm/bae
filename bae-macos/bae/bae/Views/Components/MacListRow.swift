import BaeKit
import SwiftUI

/// One row of a track list in the side pane: a leading column (a playlist
/// entry's number, a play's time), a cover, a title over a second line, then
/// whose it is and its length when given.
///
/// The leading column hangs in the margin left of the row, right-aligned
/// and `leadWidth` wide, so the covers start at the list's edge, as a
/// playlist's page and History draw their rows. `wide` leaves more room at
/// the end, for a row that ends in a person. While the owner drags a
/// playlist's row, `lifted` is the row left faint in its place and `dragging`
/// the copy under the pointer, raised over the rows.
struct MacListRow: View {
    let lead: String
    var leadWidth: CGFloat = 20
    var cover: BridgeImageRef?
    let title: String
    let subtitle: String
    /// The person the row names at its end, with their picture.
    var who: BridgePlaylistOwner?
    var duration: String?
    var wide = false
    var hover = false
    var lifted = false
    var dragging = false

    /// The space between the row's columns.
    static let gap: CGFloat = 10
    /// How far a row's leading column reaches into the margin left of its
    /// list: its width and the gap after it.
    static func hang(leadWidth: CGFloat) -> CGFloat {
        leadWidth + gap
    }

    private static var coverSize: CGFloat { 40 }
    private static var durationWidth: CGFloat { 40 }

    var body: some View {
        HStack(spacing: Self.gap) {
            Text(lead)
                .themeText(.detail)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: leadWidth, alignment: .trailing)
            Artwork(imageRef: cover, pointSize: Self.coverSize)
                .frame(width: Self.coverSize, height: Self.coverSize)
                .clipShape(
                    RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                )
            VStack(alignment: .leading, spacing: 3) {
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
            if let duration {
                Text(duration)
                    .themeText(.detail)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(width: Self.durationWidth, alignment: .trailing)
            }
        }
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
        .padding(.leading, -Self.hang(leadWidth: leadWidth))
    }

    private var fill: AnyShapeStyle {
        if dragging {
            return AnyShapeStyle(Theme.surfaceElevated)
        }
        return AnyShapeStyle(hover ? Theme.hover : .clear)
    }
}
