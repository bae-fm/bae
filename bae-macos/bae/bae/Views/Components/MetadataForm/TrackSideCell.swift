import BaeKit
import SwiftUI

/// The disc/side cell — a required `Int32`, centered tabular digits.
struct TrackSideCell: View {
    @Binding
    var value: Int32

    @FocusState
    private var focused: Bool

    var body: some View {
        TextField(value: $value, format: .number) {
            Text(verbatim: "")
        }
        .accessibilityLabel("Side")
        .focused($focused)
        .modifier(NumericCellStyle(focused: focused))
    }
}

#if DEBUG
    #Preview("Track Side Cell") {
        @Previewable
        @State
        var side: Int32 = 1
        @Previewable
        @State
        var disc: Int32 = 2
        HStack(spacing: 12) {
            TrackSideCell(value: $side).frame(width: 60)
            TrackSideCell(value: $disc).frame(width: 60)
        }
        .padding(24)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
