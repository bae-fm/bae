import BaeKit
import SwiftUI

/// Bounds cover browsers to the modal's window, including short windows.
struct CoverGalleryFrame<Content: View>: View {
    /// The space left around the browser, both sides together.
    private let margins = 2 * ThemeSpace.section

    /// The size of the window the modal opens over.
    let host: CGSize
    @ViewBuilder
    let content: () -> Content

    var body: some View {
        content()
            .frame(
                width: min(1_100, max(0, host.width - margins)),
                height: min(820, max(0, host.height - margins))
            )
    }
}
