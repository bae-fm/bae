import BaeKit
import SwiftUI

/// Shared styling for the centered, tabular borderless numeric track cells —
/// the disc and track-number columns differ only in required vs optional
/// `Int32`, so the look lives here.
struct NumericCellStyle: ViewModifier {
    let focused: Bool

    func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .font(.system(size: 13))
            .monospacedDigit()
            .multilineTextAlignment(.center)
            .modifier(FieldChrome(focused: focused, boxed: false))
    }
}

#if DEBUG
    #Preview("Numeric Cell Style") {
        // Hosts centered tabular digits where the disc/track-number fields sit —
        // resting then focused.
        HStack(spacing: 12) {
            Text("1").modifier(NumericCellStyle(focused: false))
            Text("2").modifier(NumericCellStyle(focused: true))
            Text("12").modifier(NumericCellStyle(focused: false))
        }
        .padding(24)
        .frame(width: 220)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
