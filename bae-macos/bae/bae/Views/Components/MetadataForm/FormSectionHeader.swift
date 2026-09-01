import BaeKit
import SwiftUI

/// The uppercase tracked label a metadata section or a table column leads with.
/// Section headers render at 11pt; the denser column headers take the 10pt
/// default.
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

/// A section header: the eyebrow, and an optional right-aligned note stating
/// what the section holds.
///
/// `ruled` runs a hairline from the eyebrow to the far edge. A header over a
/// bordered card has the card's edge to divide it from what came before; a
/// header over open content — a gallery, a borderless table — has the rule.
struct FormSectionHeader: View {
    let title: String
    var trailing: String?
    var ruled = false

    var body: some View {
        HStack(alignment: ruled ? .center : .firstTextBaseline, spacing: 8) {
            FormEyebrow(text: Text(verbatim: title), size: 11)
            if ruled {
                Rectangle()
                    .fill(.white.opacity(0.06))
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
    /// The grouped inset card every metadata field group and table sits in —
    /// the macOS System-Settings group idiom.
    func formGroupCard() -> some View {
        self
            .background(Theme.surface)
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .strokeBorder(.white.opacity(0.07), lineWidth: 1)
            }
    }
}
