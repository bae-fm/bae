import BaeKit
import SwiftUI

/// The artist's name over their albums in the grouped album grid.
struct MacArtistGroupHeading: View {
    let name: String

    var body: some View {
        Text(verbatim: name)
            .themeText(.rowTitle)
            .padding(.horizontal, ThemeSpace.compact)
            .accessibilityAddTraits(.isHeader)
    }
}
