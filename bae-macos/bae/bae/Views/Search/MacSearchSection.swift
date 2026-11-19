import BaeKit
import SwiftUI

/// The label that heads a section of search results, such as Albums or
/// Tracks.
struct MacSearchSection: View {
    let title: LocalizedStringKey

    init(_ title: LocalizedStringKey) {
        self.title = title
    }

    var body: some View {
        MacEyebrow(title)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, ThemeSpace.related)
            .padding(.top, ThemeSpace.group)
            .padding(.bottom, ThemeSpace.related)
    }
}
