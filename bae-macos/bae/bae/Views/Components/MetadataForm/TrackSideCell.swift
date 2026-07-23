import SwiftUI

/// The disc/side cell — a required `Int32`, centered tabular digits.
struct TrackSideCell: View {
    @Binding
    var value: Int32

    @FocusState
    private var focused: Bool

    var body: some View {
        TextField("", value: $value, format: .number)
            .focused($focused)
            .modifier(NumericCellStyle(focused: focused))
    }
}
