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
