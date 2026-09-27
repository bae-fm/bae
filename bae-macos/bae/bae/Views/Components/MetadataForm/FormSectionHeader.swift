import BaeKit
import SwiftUI

/// A section header: the eyebrow and an optional right-aligned note.
///
/// `ruled` draws a hairline to the far edge, for headers over borderless
/// content.
struct FormSectionHeader: View {
    let title: String
    var trailing: String?
    var ruled = false

    var body: some View {
        HStack(alignment: ruled ? .center : .firstTextBaseline, spacing: 8) {
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
        .padding(.horizontal, 2)
    }
}

extension View {
    /// The bordered card that metadata field groups and tables sit in.
    func formGroupCard() -> some View {
        self
            .background(Theme.surface)
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
            .overlay {
                RoundedRectangle(cornerRadius: ThemeRadius.card)
                    .strokeBorder(Theme.hairline, lineWidth: 1)
            }
    }
}
