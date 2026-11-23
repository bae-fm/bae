import BaeKit
import SwiftUI

/// Bounds cover browsers to the modal's window, including short windows.
struct CoverGalleryFrame<Content: View>: View {
    /// The space left around the browser, both sides together.
    private let margins = 2 * ThemeSpace.section

    @Environment(\.modalHost)
    private var modalHost
    @ViewBuilder
    let content: () -> Content

    var body: some View {
        content()
            .frame(
                width: min(1_100, max(0, modalHost.width - margins)),
                height: min(820, max(0, modalHost.height - margins))
            )
    }
}
