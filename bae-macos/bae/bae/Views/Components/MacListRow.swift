import BaeKit
import SwiftUI

/// One row of a track list: a leading column (a playlist entry's number, a
/// play's time), a cover, a title over a second line, then whose it is and
/// its length.
///
/// With `hanging`, the leading column hangs in the margin left of the row,
/// right-aligned and `leadWidth` wide, so the covers start at the list's
/// edge, as a playlist's page and History in the side pane draw their rows. `wide`
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

    struct Removal {
        let label: LocalizedStringKey
        let action: () -> Void
    }

    var lead: String?
    var leadWidth: CGFloat = 56
    var hanging = false
    var coverSize: CGFloat = MacListRow.defaultCoverSize
    var cover: BridgeImageRef?
    let title: String
    let subtitle: String
    /// The person the row names at its end, with their picture.
    var who: BridgePersonRef?
    /// The row's length; a play of History has none.
    var duration: String?
    var wide = false
    var hover = false
    var lift = Lift.resting
    var removal: Removal?

    /// The space between the row's columns.
    static let gap = ThemeSpace.related + ThemeSpace.line
    static let horizontalInset = ThemeSpace.related
    static let verticalInset = ThemeSpace.inline + ThemeSpace.hairline
    /// How far a hanging row's leading column reaches into the margin left of
    /// its list: its width and the gap after it.
    static func hang(leadWidth: CGFloat) -> CGFloat {
        leadWidth + gap
    }

    /// The cover's size unless a row asks for another.
    static let defaultCoverSize: CGFloat = 32
    private static let durationWidth: CGFloat = 40
    private static let removeWidth: CGFloat = 20

    var body: some View {
        HStack(spacing: Self.gap) {
            if let lead {
                Text(lead)
                    .themeText(.detail)
                    .monospacedDigit()
                    .foregroundStyle(Theme.macTextSecondary)
                    .lineLimit(1)
                    .frame(
                        width: leadWidth,
                        alignment: hanging ? .trailing : .leading
                    )
                    .accessibilitySortPriority(1)
            }
            else if hanging {
                Color.clear.frame(width: leadWidth, height: 0)
            }
            Artwork(imageRef: cover, pointSize: coverSize, cornerRadius: nil)
                .frame(width: coverSize, height: coverSize)
                .clipShape(
                    RoundedRectangle(
                        cornerRadius: coverSize == Self.defaultCoverSize
                            ? ThemeRadius.bar : ThemeRadius.artwork
                    )
                )
            VStack(
                alignment: .leading,
                spacing: hanging ? ThemeSpace.line + ThemeSpace.hairline : 0
            ) {
                Text(title)
                    .themeText(.rowTitle)
                    .lineLimit(1)
                Text(subtitle)
                    .themeText(.detail)
                    .foregroundStyle(Theme.macTextSecondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .contain)
            if let who {
                MacPersonTag(person: who)
                    .accessibilityElement(children: .contain)
                    .accessibilitySortPriority(-1)
            }
            if let duration {
                Text(duration)
                    .themeText(.detail)
                    .monospacedDigit()
                    .foregroundStyle(Theme.macTextSecondary)
                    .frame(width: Self.durationWidth, alignment: .trailing)
                    .accessibilitySortPriority(-2)
            }
        }
        .padding(.trailing, showsRemoval ? Self.removeWidth + Self.gap : 0)
        .overlay(alignment: .trailing) {
            if let removal {
                MacIconButton(
                    symbol: "xmark",
                    label: removal.label,
                    iconSize: .small,
                    hitSize: Self.removeWidth,
                    action: removal.action
                )
                .opacity(showsRemoval ? 1 : 0)
                .allowsHitTesting(showsRemoval)
                .accessibilityHidden(!showsRemoval)
                .accessibilitySortPriority(-3)
            }
        }
        .padding(.trailing, wide ? ThemeSpace.edge : Self.horizontalInset)
        .padding(
            .leading,
            hanging ? 0 : Self.horizontalInset
        )
        .padding(.vertical, Self.verticalInset)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                .fill(fill)
        )
        .contentShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
        .accessibilityElement(children: .contain)
        .opacity(lift == .lifted ? 0.45 : lift == .dragging ? 0.75 : 1)
        .shadow(
            color: lift == .dragging ? Theme.shadow : .clear,
            radius: ThemeSpace.group,
            y: ThemeSpace.related
        )
        .padding(.leading, hanging ? -Self.hang(leadWidth: leadWidth) : 0)
    }

    private var showsRemoval: Bool { removal != nil && hover }

    private var fill: AnyShapeStyle {
        if lift == .dragging {
            return AnyShapeStyle(Theme.surfaceElevated)
        }
        return AnyShapeStyle(hover ? Theme.hover : .clear)
    }
}
