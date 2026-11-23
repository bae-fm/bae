import SwiftUI

extension View {
    /// The face every Mac sheet shares: the raised colour, the panel's
    /// corners and a shadow, with no edge. `MacAlert`, `MacSidePause` and
    /// the album sheets in `MacModal` draw it.
    public func sheetFace() -> some View {
        background(Theme.surfaceElevated)
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.panel))
            .shadow(color: Theme.shadow, radius: 15, y: 5)
    }
}
