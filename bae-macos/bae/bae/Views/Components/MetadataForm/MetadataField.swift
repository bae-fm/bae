import SwiftUI

/// A `String` text field styled to the confirm-pane vocabulary: a recessed
/// well that lifts and gains an accent border on focus. `boxed` is the
/// grouped-card look (always-visible well); `boxed: false` is the track
/// table's borderless cell, transparent until focused.
struct MetadataField: View {
    let placeholder: String
    @Binding
    var text: String
    var monospaced: Bool = false
    var boxed: Bool = true

    @FocusState
    private var focused: Bool

    var body: some View {
        field
            .modifier(FieldChrome(focused: focused, boxed: boxed))
    }

    @ViewBuilder
    private var field: some View {
        let base = TextField(placeholder, text: $text)
            .textFieldStyle(.plain)
            .font(.system(size: 13))
            .focused($focused)
        if monospaced {
            base.monospacedDigit()
        }
        else {
            base
        }
    }
}
