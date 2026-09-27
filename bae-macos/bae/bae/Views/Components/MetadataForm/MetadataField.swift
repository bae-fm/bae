import BaeKit
import SwiftUI

/// A `String` text field in the shared field chrome: the grouped-card well by
/// default, or the track table's inline cell that shows its chrome only under
/// the pointer and while focused.
struct MetadataField: View {
    let placeholder: String
    @Binding
    var text: String
    var monospaced: Bool = false
    var chrome: FieldChrome.Style = .boxed

    @FocusState
    private var focused: Bool

    var body: some View {
        field
            .modifier(FieldChrome(focused: focused, style: chrome))
    }

    @ViewBuilder
    private var field: some View {
        let base = TextField(placeholder, text: $text)
            .textFieldStyle(.plain)
            .themeText(.body)
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
        VStack(spacing: ThemeSpace.group) {
            MetadataField(placeholder: "Album title", text: $albumTitle)
            MetadataField(
                placeholder: "Catalog number",
                text: $catalogNumber,
                monospaced: true
            )
            // Inline cell: transparent until hovered or focused.
            MetadataField(
                placeholder: "Track title",
                text: $borderless,
                chrome: .inline
            )
        }
        .padding(ThemeSpace.section)
        .frame(width: 300)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
