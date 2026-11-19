import BaeKit
import SwiftUI

/// The round back button before a page's heading, named Back to the page it
/// goes back to, `to`.
struct MacBackButton: View {
    let to: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "chevron.backward")
                .themeIcon(.small)
        }
        .buttonStyle(
            BoxedIconButtonStyle(size: .back)
                .accentFocus(
                    cornerRadius: BoxedIconButtonStyle.Size.back.cornerRadius
                )
        )
        .accessibilityLabel(Text("Back to \(to)"))
    }
}
