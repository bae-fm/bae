import BaeKit
import SwiftUI

/// A section header: the section's name over a full-width hairline.
struct FormSectionHeader: View {
    let title: String

    var body: some View {
        Text(verbatim: title)
            .themeText(.detail)
            .fontWeight(.semibold)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.bottom, ThemeSpace.inline)
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(Theme.hairline)
                    .frame(height: ThemeSpace.hairline)
            }
    }
}
