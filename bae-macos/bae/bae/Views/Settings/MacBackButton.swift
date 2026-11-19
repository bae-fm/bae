import BaeKit
import SwiftUI

/// The round button back to the page before, at a page's top left, named for
/// that page to VoiceOver.
struct MacBackButton: View {
    let backTitle: String
    let onBack: () -> Void

    var body: some View {
        Button(action: onBack) {
            Image(systemName: "chevron.backward")
                .themeIcon(.small)
        }
        .buttonStyle(
            BoxedIconButtonStyle(size: .back)
                .accentFocus(
                    cornerRadius: BoxedIconButtonStyle.Size.back.cornerRadius
                )
        )
        .accessibilityLabel(Text("Back to \(backTitle)"))
    }
}
