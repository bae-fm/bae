import SwiftUI

/// A rounded face in the elevated surface with its shadow below it,
/// rasterized together. A macOS window reverses the vertical offset of a
/// layer shadow, so a face drawn with SwiftUI's own shadow throws it above.
public struct ShadowedFace: View {
    private let cornerRadius: CGFloat
    private let radius: CGFloat
    private let offset: CGFloat

    /// `radius` and `offset` are the shadow's blur radius and its distance
    /// below the face.
    public init(cornerRadius: CGFloat, radius: CGFloat, offset: CGFloat) {
        self.cornerRadius = cornerRadius
        self.radius = radius
        self.offset = offset
    }

    /// How far the shadow reaches beyond the face on its widest side.
    public nonisolated static func shadowRoom(radius: CGFloat, offset: CGFloat)
        -> CGFloat
    {
        2 * radius + offset
    }

    public var body: some View {
        let shadowRoom = Self.shadowRoom(radius: radius, offset: offset)
        RoundedRectangle(cornerRadius: cornerRadius)
            .fill(Theme.surfaceElevated)
            .shadow(color: Theme.shadow, radius: radius, y: offset)
            .padding(shadowRoom)
            .drawingGroup()
            .padding(-shadowRoom)
    }
}
