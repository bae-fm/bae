import SwiftUI

/// The track-number cell — an optional `Int32?` (blank when unset),
/// centered tabular digits.
struct TrackNumberCell: View {
    @Binding
    var value: Int32?

    @FocusState
    private var focused: Bool

    var body: some View {
        TextField("", value: $value, format: .number)
            .focused($focused)
            .modifier(NumericCellStyle(focused: focused))
    }
}
