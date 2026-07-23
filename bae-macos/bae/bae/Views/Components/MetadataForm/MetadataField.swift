import BaeKit
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

#if DEBUG
    #Preview("Metadata Field") {
        @Previewable
        @State
        var albumTitle = "Album Title"
        @Previewable
        @State
        var catalogNumber = "CAT-0001"
        @Previewable
        @State
        var borderless = ""
        VStack(spacing: 12) {
            MetadataField(placeholder: "Album title", text: $albumTitle)
            MetadataField(
                placeholder: "Catalog number",
                text: $catalogNumber,
                monospaced: true
            )
            // Borderless cell: transparent until focused. Empty shows placeholder.
            MetadataField(
                placeholder: "Track title",
                text: $borderless,
                boxed: false
            )
        }
        .padding(24)
        .frame(width: 300)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
