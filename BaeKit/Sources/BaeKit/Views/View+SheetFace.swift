import SwiftUI

/// The shadow of the face every Mac sheet shares, the board's
/// `0 10px 30px`: a blur of 30 and 10 points down.
public enum SheetFaceMetrics {
    static let shadowBlur: CGFloat = 30
    static let shadowOffset: CGFloat = 10

    /// The room around a sheet that its shadow needs.
    public static let shadowRoom = ShadowedFace.shadowRoom(
        blur: shadowBlur,
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
                    blur: SheetFaceMetrics.shadowBlur,
                    offset: SheetFaceMetrics.shadowOffset
                )
            }
    }
}
