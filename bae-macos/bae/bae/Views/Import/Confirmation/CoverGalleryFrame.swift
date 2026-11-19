import BaeKit
import SwiftUI

/// Bounds cover browsers to the available modal host, including short windows.
struct CoverGalleryFrame<Content: View>: View {
    /// The space left around the browser, both sides together.
    private let margins = 2 * ThemeSpace.section

    @ViewBuilder
    let content: () -> Content

    var body: some View {
        GeometryReader { geometry in
            content()
                .frame(
                    width: min(1_100, max(0, geometry.size.width - margins)),
                    height: min(820, max(0, geometry.size.height - margins))
                )
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}
