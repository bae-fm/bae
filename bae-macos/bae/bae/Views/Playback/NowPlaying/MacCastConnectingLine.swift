import BaeKit
import SwiftUI

/// The line over the receivers while a cast connects, naming the receiver.
struct MacCastConnectingLine: View {
    let name: String

    var body: some View {
        ProgressView {
            Text(
                "Connecting to \(name)…",
                comment:
                    "Cast picker and Settings → Casting, over the receivers, and the Cast button's tooltip, while bae connects to the device the person picked; the argument is its name."
            )
        }
        .controlSize(.small)
        .themeText(.body)
        .padding(.vertical, ThemeSpace.line)
    }
}
