import BaeKit
import SwiftUI

/// A section header: the eyebrow, an optional right-aligned note, and with
/// `ruled` a hairline to the far edge for headers over borderless content.
struct FormSectionHeader: View {
    let title: String
    var trailing: String?
    var ruled = false

    var body: some View {
        HStack(
            alignment: ruled ? .center : .firstTextBaseline,
            spacing: ThemeSpace.related
        ) {
            Eyebrow(verbatim: title)
            if ruled {
                Rectangle()
                    .fill(Theme.hairline)
                    .frame(height: 1)
            }
            else {
                Spacer()
            }
            if let trailing {
                Text(trailing)
                    .themeText(.detail)
                    .monospacedDigit()
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.horizontal, ThemeSpace.line)
    }
}
