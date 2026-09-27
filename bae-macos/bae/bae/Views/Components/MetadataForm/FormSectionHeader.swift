import BaeKit
import SwiftUI

/// The uppercase label that leads a metadata section or a table column.
struct FormEyebrow: View {
    let text: Text
    var size: CGFloat = 10

    var body: some View {
        text
            .font(.system(size: size, weight: .bold))
            .textCase(.uppercase)
            .tracking(1)
            .foregroundStyle(.tertiary)
    }
}

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
            FormEyebrow(text: Text(verbatim: title), size: 11)
            if ruled {
                Rectangle()
                    .fill(Theme.hover)
                    .frame(height: 1)
            }
            else {
                Spacer()
            }
            if let trailing {
                Text(trailing)
                    .font(.system(size: 11.5))
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
