import SwiftUI

/// The system's stepper beside a count it changes, from 1 to 99, with its
/// label hidden and named by what it counts (`label`).
struct MacStepper: View {
    let label: String
    @Binding
    var value: Int

    var body: some View {
        Stepper(value: $value, in: 1...99) {
            EmptyView()
        }
        .labelsHidden()
        .accessibilityLabel(label)
    }
}
