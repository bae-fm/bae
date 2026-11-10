import SwiftUI

/// The six digits two devices both show while one lets the other in, large
/// and centred so the person can read them aloud and check they match.
struct MacMatchCode: View {
    let code: String

    var body: some View {
        Text(verbatim: code)
            .font(.system(size: 34, weight: .semibold))
            .tracking(3)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity)
    }
}
