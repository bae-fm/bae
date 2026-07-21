import SwiftUI

/// The library page's shared content container: the header band and every
/// browse mode's content center in the same width-capped column so their
/// edges align at every window width. The `libraryFullWidth` setting lifts
/// the cap for the whole page at once.
enum LibraryContentContainer {
    /// Cap on the content width; the container centers in wider windows.
    static let maxWidth: CGFloat = 1240
    /// Horizontal inset from the container edge to the content.
    static let horizontalPadding: CGFloat = 16
}

extension View {
    /// Centers this view in the library page's shared width-capped column —
    /// or spans the full width when the user lifted the cap
    /// (`Config.libraryFullWidth`).
    func libraryContentContainer(fullWidth: Bool) -> some View {
        frame(
            maxWidth: fullWidth ? .infinity : LibraryContentContainer.maxWidth
        )
        .frame(maxWidth: .infinity)
    }
}
