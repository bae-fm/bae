import SwiftUI

/// The shadow of the face every Mac sheet shares.
public enum SheetFaceMetrics {
    static let shadowRadius: CGFloat = 15
    static let shadowOffset: CGFloat = 10

    /// The room around a sheet that its shadow needs.
    public static let shadowRoom = ShadowedFace.shadowRoom(
        radius: shadowRadius,
        offset: shadowOffset
    )
}

extension View {
    /// The face every Mac sheet shares: the raised colour, the panel's
    /// corners and a shadow below, with no edge. `MacAlert`, `MacSidePause`
    /// and the album sheets in `MacModal` draw it. The face and its shadow
    /// are rasterized together, because a macOS window reverses the vertical
    /// offset of a layer shadow (`ShadowedFace`).
    public func sheetFace() -> some View {
        clipShape(RoundedRectangle(cornerRadius: ThemeRadius.panel))
            .background {
                ShadowedFace(
                    cornerRadius: ThemeRadius.panel,
                    radius: SheetFaceMetrics.shadowRadius,
                    offset: SheetFaceMetrics.shadowOffset
                )
            }
    }
}
