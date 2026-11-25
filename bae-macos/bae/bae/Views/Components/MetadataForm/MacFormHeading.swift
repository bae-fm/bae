import BaeKit
import SwiftUI

/// A section header: the section's name over a full-width hairline.
struct MacFormHeading: View {
    let title: String

    var body: some View {
        // The hairline sits under the name's padding and adds its own point
        // to the height, as the board's bottom border does.
        VStack(spacing: 0) {
            Text(verbatim: title)
                .themeText(.detail)
                .foregroundStyle(Theme.textSecondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.bottom, ThemeSpace.inline)
            MacHairline()
        }
    }
}
