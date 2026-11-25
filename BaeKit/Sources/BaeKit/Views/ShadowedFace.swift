import SwiftUI

/// A rounded face (in the elevated surface unless `fill` says otherwise) with
/// its shadow below it, rasterized together. A macOS window reverses the
/// vertical offset of a layer shadow, so a face drawn with SwiftUI's own
/// shadow throws it above.
public struct ShadowedFace: View {
    private let cornerRadius: CGFloat
    private let blur: CGFloat
    private let offset: CGFloat
    private let fill: AnyShapeStyle

    /// `blur` and `offset` are the board's `box-shadow` blur and its distance
    /// below the face.
    public init(
        cornerRadius: CGFloat,
        blur: CGFloat,
        offset: CGFloat,
        fill: AnyShapeStyle = AnyShapeStyle(Theme.surfaceElevated)
    ) {
        self.cornerRadius = cornerRadius
        self.blur = blur
        self.offset = offset
        self.fill = fill
    }

    /// The radius of SwiftUI's shadow that blurs as a board's `blur` does. A
    /// CSS blur of B is a Gaussian of deviation B / 2, and so is SwiftUI's
    /// radius of B: the pixels under a sheet's shadow in the delete-album and
    /// new-playlist shots (darker than the board's 40 points below the face,
    /// lighter beside it) show that a radius of 1.5 B blurs wider.
    private nonisolated static func radius(blur: CGFloat) -> CGFloat {
        blur
    }

    /// How far the shadow reaches beyond the face on its widest side.
    public nonisolated static func shadowRoom(blur: CGFloat, offset: CGFloat)
        -> CGFloat
    {
        2 * radius(blur: blur) + offset
    }

    public var body: some View {
        let shadowRoom = Self.shadowRoom(blur: blur, offset: offset)
        RoundedRectangle(cornerRadius: cornerRadius)
            .fill(fill)
            .shadow(
                color: Theme.shadow,
                radius: Self.radius(blur: blur),
                y: offset
            )
            .padding(shadowRoom)
            .drawingGroup()
            .padding(-shadowRoom)
    }
}
