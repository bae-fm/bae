import SwiftUI

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
                    radius: 15,
                    offset: 10
                )
            }
    }
}
