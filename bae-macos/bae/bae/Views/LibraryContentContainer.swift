import SwiftUI

/// The library page's shared content container: the header band and the
/// album grid center in the same width-capped column so their edges align
/// at every window width.
enum LibraryContentContainer {
    /// Cap on the content width; the container centers in wider windows.
    static let maxWidth: CGFloat = 1240
    /// Horizontal inset from the container edge to the content.
    static let horizontalPadding: CGFloat = 16
}
