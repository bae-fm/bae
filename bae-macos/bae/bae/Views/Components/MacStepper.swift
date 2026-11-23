import BaeKit
import SwiftUI

/// The system's stepper beside a count it changes, from 1 to 99, with its
/// label hidden and named by what it counts (`label`).
struct MacStepper: View {
    let label: String
    @Binding
    var value: Int
    /// The stepper takes no input and dims, as while the release editor
    /// reads a file.
    var off = false

    var body: some View {
        Stepper(value: $value, in: 1...99) {
            EmptyView()
        }
        .labelsHidden()
        .disabled(off)
        .opacity(off ? ThemeOpacity.off : 1)
        .accessibilityLabel(label)
    }
}
