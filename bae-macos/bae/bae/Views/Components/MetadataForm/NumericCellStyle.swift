import BaeKit
import SwiftUI

/// The look of the track table's centered, tabular disc and track-number
/// cells.
struct NumericCellStyle: ViewModifier {
    let focused: Bool

    func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .themeText(.body)
            .monospacedDigit()
            .multilineTextAlignment(.center)
            .modifier(FieldChrome(focused: focused, style: .inline))
    }
}

#if DEBUG
    #Preview("Numeric Cell Style") {
        // Resting and focused cells.
        HStack(spacing: ThemeSpace.group) {
            Text(verbatim: "1").modifier(NumericCellStyle(focused: false))
            Text(verbatim: "2").modifier(NumericCellStyle(focused: true))
            Text(verbatim: "12").modifier(NumericCellStyle(focused: false))
        }
        .padding(ThemeSpace.section)
        .frame(width: 220)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
