import BaeKit
import SwiftUI

/// The heading ("Works", "Releases", "Recordings", "Credits") above a group of
/// detail-pane rows.
struct SectionHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .themeText(.heading)
            .padding(.top, ThemeSpace.inline)
    }
}
