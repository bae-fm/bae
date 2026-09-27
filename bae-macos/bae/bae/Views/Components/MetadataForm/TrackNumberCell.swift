import BaeKit
import SwiftUI

/// The track-number cell — an optional `Int32?` (blank when unset),
/// centered tabular digits.
struct TrackNumberCell: View {
    @Binding
    var value: Int32?

    @FocusState
    private var focused: Bool

    var body: some View {
        TextField(value: $value, format: .number) {
            Text(verbatim: "")
        }
        .accessibilityLabel("Track number")
        .focused($focused)
        .modifier(NumericCellStyle(focused: focused))
    }
}

#if DEBUG
    #Preview("Track Number Cell") {
        @Previewable
        @State
        var number: Int32? = 7
        @Previewable
        @State
        var blank: Int32?
        let cellWidth: CGFloat = 60
        HStack(spacing: ThemeSpace.group) {
            TrackNumberCell(value: $number).frame(width: cellWidth)
            // Unset — renders blank.
            TrackNumberCell(value: $blank).frame(width: cellWidth)
        }
        .padding(ThemeSpace.section)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
