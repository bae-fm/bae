import BaeKit
import SwiftUI

/// The six digits two devices both show while one lets the other in, large
/// and centred so the person can read them aloud and check they match.
struct MacMatchCode: View {
    let code: String
    /// Inside a sheet, set a little further from the line above it, so the code
    /// stands apart from the words that ask to check it.
    var dialog = false

    var body: some View {
        Text(verbatim: code)
            .font(.system(size: 34, weight: .semibold))
            .tracking(3)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity)
            .padding(.top, dialog ? ThemeSpace.related : 0)
    }
}
