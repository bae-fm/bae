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

    /// How far the shadow reaches beyond the face on its widest side.
    public nonisolated static func shadowRoom(blur: CGFloat, offset: CGFloat)
        -> CGFloat
    {
        2 * blur + offset
    }

    /// A CSS blur of B is a Gaussian of deviation B / 2, which is SwiftUI's
    /// shadow radius of B.
    public var body: some View {
        let shadowRoom = Self.shadowRoom(blur: blur, offset: offset)
        RoundedRectangle(cornerRadius: cornerRadius)
            .fill(fill)
            .shadow(
                color: Theme.shadow,
                radius: blur,
                y: offset
            )
            .padding(shadowRoom)
            .drawingGroup()
            .padding(-shadowRoom)
    }
}
