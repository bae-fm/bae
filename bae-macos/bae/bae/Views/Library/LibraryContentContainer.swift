import BaeKit
import SwiftUI

/// The width-capped column the library's header and every browse mode share,
/// so their edges align at any window width.
enum LibraryContentContainer {
    /// Cap on the content width; the container centers in wider windows.
    static let maxWidth: CGFloat = 1240
    /// Horizontal inset from the container edge to the content.
    static let horizontalPadding = ThemeSpace.edge
}

extension View {
    /// Centers this view in the library's capped column, or spans the full
    /// width when `Config.libraryFullWidth` lifts the cap.
    func libraryContentContainer(fullWidth: Bool) -> some View {
        frame(
            maxWidth: fullWidth ? .infinity : LibraryContentContainer.maxWidth
        )
        .frame(maxWidth: .infinity)
    }
}
