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
struct FormSectionHeader: View {
    let title: String
    var trailing: String?

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            FormEyebrow(text: Text(verbatim: title), size: 11)
            Spacer()
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
